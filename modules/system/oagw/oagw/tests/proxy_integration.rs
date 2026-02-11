use http::{Method, StatusCode};
use oagw::test_support::{APIKEY_AUTH_PLUGIN_ID, AppHarness, parse_resource_gts};
use oagw_sdk::Body;
use oagw_sdk::api::ErrorSource;
use oagw_sdk::{
    BurstConfig, CreateRouteRequest, CreateUpstreamRequest, Endpoint, HttpMatch, HttpMethod,
    MatchRules, PathSuffixMode, RateLimitAlgorithm, RateLimitConfig, RateLimitScope,
    RateLimitStrategy, Scheme, Server, SharingMode, SustainedRate, Window,
};

async fn setup_openai_mock() -> AppHarness {
    let h = AppHarness::builder()
        .with_credentials(vec![("cred://openai-key".into(), "sk-test123".into())])
        .build()
        .await;

    let resp = h
        .api_v1()
        .post_upstream()
        .with_body(serde_json::json!({
            "server": {
                "endpoints": [{"host": "127.0.0.1", "port": h.mock_port(), "scheme": "http"}]
            },
            "protocol": "gts.x.core.oagw.protocol.v1~x.core.oagw.http.v1",
            "alias": "mock-upstream",
            "enabled": true,
            "tags": [],
            "auth": {
                "type": APIKEY_AUTH_PLUGIN_ID,
                "sharing": "private",
                "config": {
                    "header": "authorization",
                    "prefix": "Bearer ",
                    "secret_ref": "cred://openai-key"
                }
            }
        }))
        .expect_status(201)
        .await;
    let upstream_id = resp.json()["id"].as_str().unwrap().to_string();
    let (_, upstream_uuid) = parse_resource_gts(&upstream_id).unwrap();

    for (methods, path) in [
        (vec!["POST", "GET"], "/v1/chat/completions"),
        (vec!["POST"], "/v1/chat/completions/stream"),
        (vec!["GET"], "/error"),
    ] {
        h.api_v1()
            .post_route()
            .with_body(serde_json::json!({
                "upstream_id": upstream_uuid,
                "match": {
                    "http": {
                        "methods": methods,
                        "path": path
                    }
                },
                "enabled": true,
                "tags": [],
                "priority": 0
            }))
            .expect_status(201)
            .await;
    }

    h
}

// 6.13: Full pipeline — proxy POST /v1/chat/completions with JSON body.
#[tokio::test]
async fn proxy_chat_completion_round_trip() {
    let h = setup_openai_mock().await;

    let req = http::Request::builder()
        .method(Method::POST)
        .uri("/mock-upstream/v1/chat/completions")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"model":"gpt-4","messages":[{"role":"user","content":"Hello"}]}"#,
        ))
        .unwrap();
    let response = h
        .facade()
        .proxy_request(h.security_context().clone(), req)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().into_bytes().await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body_json.get("id").is_some());
    assert!(body_json.get("choices").is_some());
}

// 6.13 (auth): Verify the mock received the Authorization header.
#[tokio::test]
async fn proxy_injects_auth_header() {
    let h = setup_openai_mock().await;

    let req = http::Request::builder()
        .method(Method::POST)
        .uri("/mock-upstream/v1/chat/completions")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"model":"gpt-4","messages":[]}"#))
        .unwrap();
    let response = h
        .facade()
        .proxy_request(h.security_context().clone(), req)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let recorded = h.mock().recorded_requests().await;
    assert!(!recorded.is_empty());
    let last = &recorded[recorded.len() - 1];
    let auth_header = last
        .headers
        .iter()
        .find(|(k, _)| k == "authorization")
        .map(|(_, v)| v.as_str())
        .expect("authorization header missing");
    assert_eq!(auth_header, "Bearer sk-test123");
}

// 6.14: SSE streaming — proxy to /v1/chat/completions/stream.
#[tokio::test]
async fn proxy_sse_streaming() {
    let h = setup_openai_mock().await;

    let req = http::Request::builder()
        .method(Method::POST)
        .uri("/mock-upstream/v1/chat/completions/stream")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"model":"gpt-4","stream":true}"#))
        .unwrap();
    let response = h
        .facade()
        .proxy_request(h.security_context().clone(), req)
        .await
        .unwrap();

    let ct = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("text/event-stream"), "got content-type: {ct}");

    let body_bytes = response.into_body().into_bytes().await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("data: [DONE]"));
}

// 6.15: Upstream 500 error passthrough.
#[tokio::test]
async fn proxy_upstream_500_passthrough() {
    let h = setup_openai_mock().await;

    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/mock-upstream/error/500")
        .body(Body::Empty)
        .unwrap();
    let response = h
        .facade()
        .proxy_request(h.security_context().clone(), req)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response.extensions().get::<ErrorSource>().copied(),
        Some(ErrorSource::Upstream)
    );
}

// 6.17: Pipeline abort — nonexistent alias returns 404 without calling mock.
#[tokio::test]
async fn proxy_nonexistent_alias_returns_404() {
    let h = setup_openai_mock().await;

    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/nonexistent/v1/test")
        .body(Body::Empty)
        .unwrap();
    match h
        .facade()
        .proxy_request(h.security_context().clone(), req)
        .await
    {
        Err(err) => assert!(matches!(
            err,
            oagw_sdk::error::ServiceGatewayError::RouteNotFound { .. }
        )),
        Ok(_) => panic!("expected error"),
    }
}

// 6.17: Pipeline abort — disabled upstream returns 503.
#[tokio::test]
async fn proxy_disabled_upstream_returns_503() {
    let h = setup_openai_mock().await;
    let ctx = h.security_context().clone();

    let _upstream = h
        .facade()
        .create_upstream(
            ctx.clone(),
            CreateUpstreamRequest::builder(
                Server {
                    endpoints: vec![Endpoint {
                        scheme: Scheme::Http,
                        host: "127.0.0.1".into(),
                        port: 9999,
                    }],
                },
                "gts.x.core.oagw.protocol.v1~x.core.oagw.http.v1",
            )
            .alias("disabled-upstream")
            .enabled(false)
            .build(),
        )
        .await
        .unwrap();

    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/disabled-upstream/test")
        .body(Body::Empty)
        .unwrap();
    match h.facade().proxy_request(ctx.clone(), req).await {
        Err(err) => assert!(matches!(
            err,
            oagw_sdk::error::ServiceGatewayError::UpstreamDisabled { .. }
        )),
        Ok(_) => panic!("expected error"),
    }
}

// 6.17: Pipeline abort — rate limit exceeded returns 429.
#[tokio::test]
async fn proxy_rate_limit_exceeded_returns_429() {
    let h = AppHarness::builder().build().await;
    let ctx = h.security_context().clone();

    let upstream = h
        .facade()
        .create_upstream(
            ctx.clone(),
            CreateUpstreamRequest::builder(
                Server {
                    endpoints: vec![Endpoint {
                        scheme: Scheme::Http,
                        host: "127.0.0.1".into(),
                        port: h.mock_port(),
                    }],
                },
                "gts.x.core.oagw.protocol.v1~x.core.oagw.http.v1",
            )
            .alias("rate-limited")
            .rate_limit(RateLimitConfig {
                sharing: SharingMode::Private,
                algorithm: RateLimitAlgorithm::TokenBucket,
                sustained: SustainedRate {
                    rate: 1,
                    window: Window::Minute,
                },
                burst: Some(BurstConfig { capacity: 1 }),
                scope: RateLimitScope::Tenant,
                strategy: RateLimitStrategy::Reject,
                cost: 1,
            })
            .build(),
        )
        .await
        .unwrap();

    h.facade()
        .create_route(
            ctx.clone(),
            CreateRouteRequest::builder(
                upstream.id,
                MatchRules {
                    http: Some(HttpMatch {
                        methods: vec![HttpMethod::Get],
                        path: "/v1/models".into(),
                        query_allowlist: vec![],
                        path_suffix_mode: PathSuffixMode::Append,
                    }),
                    grpc: None,
                },
            )
            .build(),
        )
        .await
        .unwrap();

    // First request should succeed.
    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/rate-limited/v1/models")
        .body(Body::Empty)
        .unwrap();
    let response = h.facade().proxy_request(ctx.clone(), req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Second request should be rate limited.
    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/rate-limited/v1/models")
        .body(Body::Empty)
        .unwrap();
    match h.facade().proxy_request(ctx.clone(), req).await {
        Err(err) => assert!(matches!(
            err,
            oagw_sdk::error::ServiceGatewayError::RateLimitExceeded { .. }
        )),
        Ok(_) => panic!("expected rate limit error"),
    }
}

// 6.16: Upstream timeout — proxy to /error/timeout with short timeout, assert 504.
#[tokio::test]
async fn proxy_upstream_timeout_returns_504() {
    let h = AppHarness::builder()
        .with_request_timeout(std::time::Duration::from_millis(500))
        .build()
        .await;
    let ctx = h.security_context().clone();

    let upstream = h
        .facade()
        .create_upstream(
            ctx.clone(),
            CreateUpstreamRequest::builder(
                Server {
                    endpoints: vec![Endpoint {
                        scheme: Scheme::Http,
                        host: "127.0.0.1".into(),
                        port: h.mock_port(),
                    }],
                },
                "gts.x.core.oagw.protocol.v1~x.core.oagw.http.v1",
            )
            .alias("timeout-upstream")
            .build(),
        )
        .await
        .unwrap();

    h.facade()
        .create_route(
            ctx.clone(),
            CreateRouteRequest::builder(
                upstream.id,
                MatchRules {
                    http: Some(HttpMatch {
                        methods: vec![HttpMethod::Get],
                        path: "/error".into(),
                        query_allowlist: vec![],
                        path_suffix_mode: PathSuffixMode::Append,
                    }),
                    grpc: None,
                },
            )
            .build(),
        )
        .await
        .unwrap();

    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/timeout-upstream/error/timeout")
        .body(Body::Empty)
        .unwrap();
    match h.facade().proxy_request(ctx.clone(), req).await {
        Err(err) => assert!(matches!(
            err,
            oagw_sdk::error::ServiceGatewayError::RequestTimeout { .. }
        )),
        Ok(_) => panic!("expected timeout error"),
    }
}

// 8.9: Query allowlist enforcement.
#[tokio::test]
async fn proxy_query_allowlist_allowed_param_succeeds() {
    let h = AppHarness::builder().build().await;
    let ctx = h.security_context().clone();

    let upstream = h
        .facade()
        .create_upstream(
            ctx.clone(),
            CreateUpstreamRequest::builder(
                Server {
                    endpoints: vec![Endpoint {
                        scheme: Scheme::Http,
                        host: "127.0.0.1".into(),
                        port: h.mock_port(),
                    }],
                },
                "gts.x.core.oagw.protocol.v1~x.core.oagw.http.v1",
            )
            .alias("ql-test")
            .build(),
        )
        .await
        .unwrap();

    h.facade()
        .create_route(
            ctx.clone(),
            CreateRouteRequest::builder(
                upstream.id,
                MatchRules {
                    http: Some(HttpMatch {
                        methods: vec![HttpMethod::Get],
                        path: "/v1/models".into(),
                        query_allowlist: vec!["version".into()],
                        path_suffix_mode: PathSuffixMode::Append,
                    }),
                    grpc: None,
                },
            )
            .build(),
        )
        .await
        .unwrap();

    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/ql-test/v1/models?version=2")
        .body(Body::Empty)
        .unwrap();
    let response = h.facade().proxy_request(ctx.clone(), req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn proxy_query_allowlist_unknown_param_rejected() {
    let h = AppHarness::builder().build().await;
    let ctx = h.security_context().clone();

    let upstream = h
        .facade()
        .create_upstream(
            ctx.clone(),
            CreateUpstreamRequest::builder(
                Server {
                    endpoints: vec![Endpoint {
                        scheme: Scheme::Http,
                        host: "127.0.0.1".into(),
                        port: h.mock_port(),
                    }],
                },
                "gts.x.core.oagw.protocol.v1~x.core.oagw.http.v1",
            )
            .alias("ql-reject")
            .build(),
        )
        .await
        .unwrap();

    h.facade()
        .create_route(
            ctx.clone(),
            CreateRouteRequest::builder(
                upstream.id,
                MatchRules {
                    http: Some(HttpMatch {
                        methods: vec![HttpMethod::Get],
                        path: "/v1/models".into(),
                        query_allowlist: vec!["version".into()],
                        path_suffix_mode: PathSuffixMode::Append,
                    }),
                    grpc: None,
                },
            )
            .build(),
        )
        .await
        .unwrap();

    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/ql-reject/v1/models?version=2&debug=true")
        .body(Body::Empty)
        .unwrap();
    match h.facade().proxy_request(ctx.clone(), req).await {
        Err(err) => assert!(matches!(
            err,
            oagw_sdk::error::ServiceGatewayError::ValidationError { .. }
        )),
        Ok(_) => panic!("expected validation error"),
    }
}

// 13.5: Non-existent auth plugin ID returns error through proxy pipeline.
#[tokio::test]
async fn proxy_nonexistent_auth_plugin_returns_error() {
    let h = AppHarness::builder().build().await;
    let ctx = h.security_context().clone();

    let upstream = h
        .facade()
        .create_upstream(
            ctx.clone(),
            CreateUpstreamRequest::builder(
                Server {
                    endpoints: vec![Endpoint {
                        scheme: Scheme::Http,
                        host: "127.0.0.1".into(),
                        port: h.mock_port(),
                    }],
                },
                "gts.x.core.oagw.protocol.v1~x.core.oagw.http.v1",
            )
            .alias("bad-auth")
            .auth(oagw_sdk::AuthConfig {
                plugin_type: "gts.x.core.oagw.auth.v1~nonexistent.plugin.v1".into(),
                sharing: SharingMode::Private,
                config: None,
            })
            .build(),
        )
        .await
        .unwrap();

    h.facade()
        .create_route(
            ctx.clone(),
            CreateRouteRequest::builder(
                upstream.id,
                MatchRules {
                    http: Some(HttpMatch {
                        methods: vec![HttpMethod::Get],
                        path: "/v1/test".into(),
                        query_allowlist: vec![],
                        path_suffix_mode: PathSuffixMode::Append,
                    }),
                    grpc: None,
                },
            )
            .build(),
        )
        .await
        .unwrap();

    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/bad-auth/v1/test")
        .body(Body::Empty)
        .unwrap();
    match h.facade().proxy_request(ctx.clone(), req).await {
        Err(err) => assert!(matches!(
            err,
            oagw_sdk::error::ServiceGatewayError::AuthenticationFailed { .. }
        )),
        Ok(_) => panic!("expected authentication error for non-existent plugin"),
    }
}

// 13.6: Assert on recorded_requests() URI and body content.
#[tokio::test]
async fn proxy_recorded_request_has_correct_uri_and_body() {
    let h = setup_openai_mock().await;

    let body_payload = r#"{"model":"gpt-4","messages":[{"role":"user","content":"test"}]}"#;
    let req = http::Request::builder()
        .method(Method::POST)
        .uri("/mock-upstream/v1/chat/completions")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(body_payload))
        .unwrap();
    let response = h
        .facade()
        .proxy_request(h.security_context().clone(), req)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let recorded = h.mock().recorded_requests().await;
    assert!(!recorded.is_empty());
    let last = &recorded[recorded.len() - 1];
    assert_eq!(last.uri, "/v1/chat/completions");
    assert_eq!(last.method, "POST");

    let body_str = String::from_utf8(last.body.clone()).unwrap();
    assert!(body_str.contains("gpt-4"));
    assert!(body_str.contains("test"));
}

// 8.10: path_suffix_mode=disabled rejects suffix; append succeeds.
#[tokio::test]
async fn proxy_path_suffix_disabled_rejects_extra_path() {
    let h = AppHarness::builder().build().await;
    let ctx = h.security_context().clone();

    let upstream = h
        .facade()
        .create_upstream(
            ctx.clone(),
            CreateUpstreamRequest::builder(
                Server {
                    endpoints: vec![Endpoint {
                        scheme: Scheme::Http,
                        host: "127.0.0.1".into(),
                        port: h.mock_port(),
                    }],
                },
                "gts.x.core.oagw.protocol.v1~x.core.oagw.http.v1",
            )
            .alias("psm-test")
            .build(),
        )
        .await
        .unwrap();

    h.facade()
        .create_route(
            ctx.clone(),
            CreateRouteRequest::builder(
                upstream.id,
                MatchRules {
                    http: Some(HttpMatch {
                        methods: vec![HttpMethod::Get],
                        path: "/v1/models".into(),
                        query_allowlist: vec![],
                        path_suffix_mode: PathSuffixMode::Disabled,
                    }),
                    grpc: None,
                },
            )
            .build(),
        )
        .await
        .unwrap();

    // Exact path succeeds.
    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/psm-test/v1/models")
        .body(Body::Empty)
        .unwrap();
    let response = h.facade().proxy_request(ctx.clone(), req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Extra suffix rejected with 400.
    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/psm-test/v1/models/gpt-4")
        .body(Body::Empty)
        .unwrap();
    match h.facade().proxy_request(ctx.clone(), req).await {
        Err(err) => assert!(matches!(
            err,
            oagw_sdk::error::ServiceGatewayError::ValidationError { .. }
        )),
        Ok(_) => panic!("expected validation error for disabled path_suffix_mode"),
    }
}
