#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
//! Proc-macros for the `modkit-gts` crate.
//!
//! Provides two macros that hide `struct_to_gts_schema` + `inventory::submit!`
//! boilerplate for GTS types shipped via the `modkit-gts` inventory:
//!
//! - **`#[gts_schema(...)]`** — attribute macro applied to a struct. Expands to
//!   a `#[gts_macros::struct_to_gts_schema(...)]` attribute (with
//!   `dir_path = "schemas"` hardcoded and `base = true` as the default) plus an
//!   `inventory::submit! { InventorySchema { ... } }` block that registers the
//!   type's schema at link time.
//!
//! - **`gts_instance! { ... }`** — function-like macro for declaring a
//!   well-known GTS instance. Auto-derives `type_id` from `instance_id` per
//!   the GTS spec (§2.2 / §3.7) chain semantics and auto-injects the `id`
//!   field into the payload. Emits an `inventory::submit! { InventoryInstance { ... } }`
//!   block.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream, Parser as _};
use syn::punctuated::Punctuated;
use syn::{ExprStruct, FieldValue, Ident, ItemStruct, LitStr, Path, Token, parse_macro_input};

const MODKIT_GTS_PKG: &str = "cf-modkit-gts";
const MODKIT_GTS_LIB: &str = "modkit_gts";

/// Resolves the path to the `modkit_gts` crate at the expansion site.
///
/// Mirrors `modkit-canonical-errors-macro`'s resolution: inside the
/// `modkit_gts` crate itself (integration tests), returns the lib name;
/// otherwise delegates to `proc_macro_crate`.
fn resolve_crate_path() -> syn::Result<TokenStream2> {
    let in_self = std::env::var("CARGO_PKG_NAME").is_ok_and(|p| p == MODKIT_GTS_PKG);
    if in_self {
        let is_lib = std::env::var("CARGO_CRATE_NAME").is_ok_and(|c| c == MODKIT_GTS_LIB);
        if is_lib {
            return Ok(quote!(crate));
        }
        let ident = Ident::new(MODKIT_GTS_LIB, proc_macro2::Span::call_site());
        return Ok(quote!(::#ident));
    }

    match proc_macro_crate::crate_name(MODKIT_GTS_PKG) {
        Ok(proc_macro_crate::FoundCrate::Itself) => Ok(quote!(crate)),
        Ok(proc_macro_crate::FoundCrate::Name(n)) => {
            let pkg_normalized = MODKIT_GTS_PKG.replace('-', "_");
            let effective = if n == pkg_normalized {
                MODKIT_GTS_LIB
            } else {
                &n
            };
            let ident = Ident::new(effective, proc_macro2::Span::call_site());
            Ok(quote!(::#ident))
        }
        Err(_) => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "cf-modkit-gts must be a direct dependency",
        )),
    }
}

// =====================================================================
//                            #[gts_schema(...)]
// =====================================================================

/// Value of the `base` attribute: `true` (default) or a path to a parent type.
enum BaseArg {
    True,
    Parent(Path),
}

struct GtsSchemaArgs {
    schema_id: LitStr,
    description: Option<LitStr>,
    properties: Option<LitStr>,
    base: BaseArg,
}

impl Parse for GtsSchemaArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut schema_id: Option<LitStr> = None;
        let mut description: Option<LitStr> = None;
        let mut properties: Option<LitStr> = None;
        let mut base: Option<BaseArg> = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let key_str = key.to_string();
            match key_str.as_str() {
                "schema_id" => schema_id = Some(input.parse()?),
                "description" => description = Some(input.parse()?),
                "properties" => properties = Some(input.parse()?),
                "base" => {
                    // Accept `base = true` or `base = ParentStruct`.
                    if input.peek(syn::LitBool) {
                        let b: syn::LitBool = input.parse()?;
                        if !b.value {
                            return Err(syn::Error::new_spanned(
                                b,
                                "base = false is not supported; omit `base` or use `base = ParentStruct`",
                            ));
                        }
                        base = Some(BaseArg::True);
                    } else {
                        let p: Path = input.parse()?;
                        base = Some(BaseArg::Parent(p));
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        format!(
                            "unknown attribute `{key_str}`; expected one of: schema_id, description, properties, base"
                        ),
                    ));
                }
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let schema_id = schema_id.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "missing `schema_id = \"...\"`",
            )
        })?;

        Ok(Self {
            schema_id,
            description,
            properties,
            base: base.unwrap_or(BaseArg::True),
        })
    }
}

/// `#[gts_schema(schema_id = "...", [description = "..."], [properties = "..."], [base = true|ParentStruct])]`
///
/// Attribute macro that marks a Rust struct as a GTS schema shipped through
/// the `modkit-gts` inventory. Expands into a call to the upstream
/// `gts_macros::struct_to_gts_schema` macro with `dir_path = "schemas"` and
/// `base = true` (or the provided parent) preset, followed by an
/// `inventory::submit!` block that registers the type's schema accessor into
/// the `InventorySchema` collector.
#[proc_macro_attribute]
pub fn gts_schema(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as GtsSchemaArgs);
    let input = parse_macro_input!(item as ItemStruct);
    match expand_gts_schema(&args, &input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_gts_schema(args: &GtsSchemaArgs, input: &ItemStruct) -> syn::Result<TokenStream2> {
    let crate_path = resolve_crate_path()?;
    let struct_name = &input.ident;
    let schema_id_lit = &args.schema_id;

    // Build the arguments for the upstream struct_to_gts_schema macro.
    let base_tokens = match &args.base {
        BaseArg::True => quote! { base = true },
        BaseArg::Parent(p) => quote! { base = #p },
    };
    let description_tokens = args
        .description
        .as_ref()
        .map(|d| quote! { , description = #d });
    let properties_tokens = args
        .properties
        .as_ref()
        .map(|p| quote! { , properties = #p });

    // Determine schema_fn call form: generic struct needs turbofish `::<()>`.
    let has_generics = input.generics.type_params().next().is_some();
    let schema_fn_body = if has_generics {
        quote! { <#struct_name::<()>>::gts_schema_with_refs_as_string() }
    } else {
        quote! { <#struct_name>::gts_schema_with_refs_as_string() }
    };

    // Auto-emit `impl Default` for derived unit structs (i.e. `base = Parent`
    // with no fields). These are pure type-markers whose sole inhabitant is
    // `Self`, so `Default` is unambiguous and lets generic helpers like
    // `PluginV1::<P>::build_registration` build the marker internally
    // without forcing the caller to repeat the type name.
    //
    // Intentionally NOT emitted when `base = true` (base types are more
    // likely to have a meaningful custom `Default`) or when the struct has
    // generics/fields (`Default` derivation then needs bounds we don't
    // want to guess).
    let is_derived_unit = matches!(input.fields, syn::Fields::Unit)
        && matches!(&args.base, BaseArg::Parent(_))
        && !has_generics;
    let default_impl = if is_derived_unit {
        // Upstream `struct_to_gts_schema` rewrites `pub struct X;` into
        // `pub struct X {}` (named-empty), so the `Self` constructor
        // doesn't compile. `Self {}` is valid for both unit and
        // named-empty shapes.
        quote! {
            impl ::core::default::Default for #struct_name {
                #[inline]
                fn default() -> Self { Self {} }
            }
        }
    } else {
        quote!()
    };

    // Emit: upstream macro attribute + original struct + inventory submission.
    Ok(quote! {
        #[::gts_macros::struct_to_gts_schema(
            dir_path = "schemas",
            #base_tokens,
            schema_id = #schema_id_lit
            #description_tokens
            #properties_tokens
        )]
        #input

        #default_impl

        #crate_path::inventory::submit! {
            #crate_path::InventorySchema {
                schema_id: #schema_id_lit,
                schema_fn: || #schema_fn_body,
            }
        }
    })
}

// =====================================================================
//                          gts_instance! { ... }
// =====================================================================

/// Derive `(type_id, segment)` from an `instance_id` string literal.
///
/// Rule (per GTS spec §2.2 / §3.7): `type_id` = prefix up to and including the
/// last `~`; `segment` = everything after. Instance id must contain at least
/// one `~` and must NOT end in `~` (that form denotes a schema).
fn split_instance_id(instance_id: &LitStr) -> syn::Result<(String, String)> {
    let raw = instance_id.value();
    if raw.ends_with('~') {
        return Err(syn::Error::new_spanned(
            instance_id,
            "instance_id must not end with `~` (that denotes a schema, not an instance)",
        ));
    }
    let Some(last_tilde) = raw.rfind('~') else {
        return Err(syn::Error::new_spanned(
            instance_id,
            "instance_id must contain at least one `~` (chained form: gts.<type>~<instance>)",
        ));
    };
    let type_id = raw[..=last_tilde].to_owned();
    let segment = raw[last_tilde + 1..].to_owned();
    Ok((type_id, segment))
}

/// Validate a bare `segment` literal. Rejects empty / starting with `~` /
/// ending in `~`. Interior `~` is allowed (lets consumers reach deeper
/// levels of derivation when needed).
fn validate_segment(segment: &LitStr) -> syn::Result<()> {
    let raw = segment.value();
    if raw.is_empty() {
        return Err(syn::Error::new_spanned(
            segment,
            "segment must not be empty",
        ));
    }
    if raw.starts_with('~') {
        return Err(syn::Error::new_spanned(
            segment,
            "segment must not start with `~` (it is concatenated onto the schema prefix)",
        ));
    }
    if raw.ends_with('~') {
        return Err(syn::Error::new_spanned(
            segment,
            "segment must not end with `~` (that would denote a schema, not an instance)",
        ));
    }
    Ok(())
}

/// Typed form of GTS instance declaration.
///
/// Submits the instance into the process-wide `InventoryInstance` collector
/// at link time. `id` is auto-injected into the struct literal from the
/// schema prefix + `segment`. Passing `id:` explicitly is rejected.
///
/// ```ignore
/// gts_instance! {
///     segment = "cf.mini_chat._.chat_read.v1",
///     instance = AuthzPermissionV1 {
///         resource_type: "...".to_owned(),
///         action: "read".to_owned(),
///         display_name: "Read chat".to_owned(),
///     }
/// }
/// ```
///
/// The full instance id is assembled at compile time as
/// `<Struct as GtsSchema>::SCHEMA_ID + segment` via `const_format::concatcp!`.
///
/// ## `schema = Type` override
///
/// Use `schema = Type` when the **schema that the instance conforms to**
/// and the **struct the macro serialises** are different types. This
/// happens whenever the conforming schema is a derived GTS type (`base =
/// Parent`) — derived types don't implement `serde::Serialize` on purpose
/// (they'd produce JSON without the base's fields), so the caller has to
/// wrap through a shallower base in `instance = ...` for serialisation.
/// Meanwhile, the instance's `id` prefix must reflect the deeper
/// conforming schema, so we spell that type out in `schema = ...`.
///
/// **When it is NOT needed.** If the struct in `instance = ...` is itself
/// the schema the instance conforms to (base type carrying all the
/// payload — typical case for flat base schemas like `AuthzPermissionV1`),
/// omit `schema`. The macro will use `<Struct as GtsSchema>::SCHEMA_ID`
/// directly. No rule of thumb about "always set schema for derived
/// chains" — only set it when the two roles genuinely differ.
///
/// **Worked example.** Given a 3-level chain:
///
/// ```ignore
/// // Level 1 (base, has `Serialize`):
/// pub struct BaseEventV1<P: GtsSchema> { pub id: GtsInstanceId, pub payload: P }
///
/// // Level 2 (derived, only `GtsSerialize`):
/// //   base = BaseEventV1,
/// //   schema_id = "gts.A.v1~B.v1~"
/// pub struct AuditPayloadV1<D: GtsSchema> { pub category: String, pub data: D }
///
/// // Level 3 (derived, only `GtsSerialize`):
/// //   base = AuditPayloadV1,
/// //   schema_id = "gts.A.v1~B.v1~C.v1~"
/// pub struct PlaceOrderDataV1 { pub order_id: String, pub amount: i64 }
/// ```
///
/// An instance carries `order_id` / `amount` — i.e. it conforms to
/// `PlaceOrderDataV1` (level-3 schema). Declaration:
///
/// ```ignore
/// gts_instance! {
///     segment = "acme.orders.place_order_01.v1",
///     schema = PlaceOrderDataV1,                          // level-3 SCHEMA_ID → 4-segment instance_id
///     instance = BaseEventV1::<AuditPayloadV1<PlaceOrderDataV1>> {
///         payload: AuditPayloadV1::<PlaceOrderDataV1> {
///             category: "orders".to_owned(),
///             data: PlaceOrderDataV1 {
///                 order_id: "o1".to_owned(),
///                 amount: 42,
///             },
///         },
///     }
/// }
/// ```
///
/// Resulting full instance id:
/// `gts.A.v1~B.v1~C.v1~acme.orders.place_order_01.v1`.
///
/// If you tried `schema = AuditPayloadV1<PlaceOrderDataV1>` instead,
/// you'd get the **level-2** `SCHEMA_ID` (`gts.A.v1~B.v1~`) as prefix —
/// the Rust generic parameter does **not** bump a type's `SCHEMA_ID`;
/// that const is a fixed literal on the outermost type. Always pick the
/// `schema = ...` type by which GTS schema the content fulfils, not by
/// which generic shape Rust required to construct it.
///
/// ## `name = IDENT` (optional — typed runtime access)
///
/// Emits `pub static NAME: LazyLock<T>` alongside the inventory submission,
/// letting callers reach the typed Rust value at runtime without parsing
/// the inventory JSON:
///
/// ```ignore
/// gts_instance! {
///     name = CHAT_READ_PERM,
///     segment = "cf.mini_chat._.chat_read.v1",
///     instance = AuthzPermissionV1 { ... }
/// }
/// // Elsewhere:
/// let p: &AuthzPermissionV1 = &CHAT_READ_PERM;
/// ```
///
/// ## For raw-JSON payloads, use [`gts_instance_raw!`] instead.
#[proc_macro]
pub fn gts_instance(input: TokenStream) -> TokenStream {
    match expand_gts_instance_typed(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Raw-JSON form of GTS instance declaration.
///
/// Use when the instance does not correspond to a Rust struct (schema is
/// declared JSON-only, or depth exceeds what typed-form ergonomics handles
/// cleanly). `id` is auto-injected into the payload JSON from `instance_id`.
///
/// ```ignore
/// gts_instance_raw! {
///     instance_id = "gts.cf.core.events.topic.v1~cf.core._.audit.v1",
///     payload = { "name": "audit", "description": "Audit log events" }
/// }
/// ```
///
/// Validation of the payload against the schema happens at
/// `types-registry::switch_to_ready()` (full JSON Schema validation). The
/// raw form intentionally gives up compile-time field checking.
#[proc_macro]
pub fn gts_instance_raw(input: TokenStream) -> TokenStream {
    match expand_gts_instance_raw(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Strips `::` from angle-bracketed generic arguments in a path so it can
/// be used in *type* position. Turbofish (`Foo::<T>`) is valid in
/// expression position; type annotations use `Foo<T>` (no `::`).
fn path_for_type_position(path: &Path) -> Path {
    let mut cloned = path.clone();
    for seg in &mut cloned.segments {
        if let syn::PathArguments::AngleBracketed(args) = &mut seg.arguments {
            args.colon2_token = None;
        }
    }
    cloned
}

fn expand_gts_instance_typed(input: TokenStream2) -> syn::Result<TokenStream2> {
    struct Args {
        /// `segment = "<tail>"` — the instance-id tail past the schema's
        /// `~`. Full id is assembled at compile time as
        /// `<schema_path as GtsSchema>::SCHEMA_ID + segment`.
        segment: LitStr,
        instance: ExprStruct,
        /// Optional explicit schema type for the prefix. When set, its
        /// `SCHEMA_ID` is used instead of the `instance` struct's own.
        /// Needed for level-3+ instance ids: the serialised struct must be
        /// a base (so `serde::Serialize` is derived), but the instance
        /// conforms to a deeper derived schema.
        schema_override: Option<Path>,
        /// Optional `name = IDENT` — emits a `pub static NAME: LazyLock<T>`
        /// next to the inventory submission for typed runtime access.
        name: Option<Ident>,
    }

    let args = (|tokens: ParseStream<'_>| -> syn::Result<Args> {
        let mut segment: Option<LitStr> = None;
        let mut instance: Option<ExprStruct> = None;
        let mut schema_override: Option<Path> = None;
        let mut name: Option<Ident> = None;

        while !tokens.is_empty() {
            let key: Ident = tokens.parse()?;
            tokens.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "segment" => {
                    if segment.is_some() {
                        return Err(syn::Error::new_spanned(&key, "duplicate `segment = ...`"));
                    }
                    segment = Some(tokens.parse()?);
                }
                "instance" => {
                    if instance.is_some() {
                        return Err(syn::Error::new_spanned(&key, "duplicate `instance = ...`"));
                    }
                    instance = Some(tokens.parse()?);
                }
                "schema" => {
                    if schema_override.is_some() {
                        return Err(syn::Error::new_spanned(&key, "duplicate `schema = ...`"));
                    }
                    schema_override = Some(tokens.parse()?);
                }
                "name" => {
                    if name.is_some() {
                        return Err(syn::Error::new_spanned(&key, "duplicate `name = ...`"));
                    }
                    name = Some(tokens.parse()?);
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        format!(
                            "unknown attribute `{other}`; expected `segment`, `instance`, `schema`, or `name`"
                        ),
                    ));
                }
            }
            if tokens.peek(Token![,]) {
                tokens.parse::<Token![,]>()?;
            }
        }

        let segment = segment.ok_or_else(|| tokens.error("missing `segment = \"...\"`"))?;
        let instance =
            instance.ok_or_else(|| tokens.error("missing `instance = SchemaStruct { ... }`"))?;
        Ok(Args {
            segment,
            instance,
            schema_override,
            name,
        })
    })
    .parse2(input)?;

    let crate_path = resolve_crate_path()?;

    validate_segment(&args.segment)?;

    // Prefix source: explicit `schema = Type` override if given, else the
    // `instance` struct's own path. Override is required for level-3+
    // instances — the struct path must be a base type (to satisfy
    // `serde::Serialize`) while the id's prefix must be a deeper derived
    // schema.
    let schema_path_owned;
    let schema_path: &Path = if let Some(explicit) = args.schema_override.as_ref() {
        explicit
    } else {
        schema_path_owned = args.instance.path.clone();
        &schema_path_owned
    };
    // Const tokens yielding `&'static str`. Land in the `InventoryInstance`
    // static fields; `segment_expr` builds the auto-injected `id`.
    let segment_lit = &args.segment;
    let type_id_expr = quote! {
        <#schema_path as #crate_path::GtsSchema>::SCHEMA_ID
    };
    let instance_id_expr = quote! {
        #crate_path::const_format::concatcp!(#type_id_expr, #segment_lit)
    };
    let segment_expr = quote! { #segment_lit };

    // Validate and rewrite the struct literal: inject `id`, reject
    // `..rest` and explicit `id:`.
    let mut struct_expr = args.instance;
    if let Some(rest) = &struct_expr.rest {
        return Err(syn::Error::new_spanned(
            rest,
            "struct update syntax (`..rest`) is not supported; list all fields explicitly",
        ));
    }
    for field in &struct_expr.fields {
        if let syn::Member::Named(ident) = &field.member
            && ident == "id"
        {
            return Err(syn::Error::new_spanned(
                field,
                "do not specify `id:` - it is auto-injected",
            ));
        }
    }
    let id_field: FieldValue = syn::parse_quote! {
        id: #crate_path::GtsInstanceId::new(#type_id_expr, #segment_expr)
    };
    let mut new_fields = Punctuated::new();
    new_fields.push(id_field);
    for f in struct_expr.fields {
        new_fields.push(f);
    }
    struct_expr.fields = new_fields;

    let submit = quote! {
        #crate_path::inventory::submit! {
            #crate_path::InventoryInstance {
                type_id: #type_id_expr,
                instance_id: #instance_id_expr,
                payload_fn: || ::serde_json::to_value(&#struct_expr)
                    .expect("GTS instance struct must serialize cleanly"),
            }
        }
    };

    // Optional typed runtime accessor: `pub static NAME: LazyLock<T> = ...`.
    // `T` is the struct path in type position (turbofish stripped).
    let static_binding = if let Some(name) = args.name {
        let type_path = path_for_type_position(&struct_expr.path);
        quote! {
            #[allow(non_upper_case_globals)]
            pub static #name: ::std::sync::LazyLock<#type_path> =
                ::std::sync::LazyLock::new(|| #struct_expr);
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #submit
        #static_binding
    })
}

fn expand_gts_instance_raw(input: TokenStream2) -> syn::Result<TokenStream2> {
    struct Args {
        instance_id: LitStr,
        payload: TokenStream2,
    }

    let args = (|tokens: ParseStream<'_>| -> syn::Result<Args> {
        let mut instance_id: Option<LitStr> = None;
        let mut payload: Option<TokenStream2> = None;

        while !tokens.is_empty() {
            let key: Ident = tokens.parse()?;
            tokens.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "instance_id" => {
                    if instance_id.is_some() {
                        return Err(syn::Error::new_spanned(
                            &key,
                            "duplicate `instance_id = ...`",
                        ));
                    }
                    instance_id = Some(tokens.parse()?);
                }
                "payload" => {
                    if payload.is_some() {
                        return Err(syn::Error::new_spanned(&key, "duplicate `payload = ...`"));
                    }
                    let content;
                    let _ = syn::braced!(content in tokens);
                    payload = Some(content.parse()?);
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        format!("unknown attribute `{other}`; expected `instance_id` or `payload`"),
                    ));
                }
            }
            if tokens.peek(Token![,]) {
                tokens.parse::<Token![,]>()?;
            }
        }

        let instance_id =
            instance_id.ok_or_else(|| tokens.error("missing `instance_id = \"...\"`"))?;
        let payload = payload.ok_or_else(|| tokens.error("missing `payload = { ... }`"))?;
        Ok(Args {
            instance_id,
            payload,
        })
    })
    .parse2(input)?;

    let (type_id, _segment) = split_instance_id(&args.instance_id)?;
    let type_id_lit = LitStr::new(&type_id, args.instance_id.span());
    let instance_id_lit = &args.instance_id;
    let payload_tokens = &args.payload;

    let crate_path = resolve_crate_path()?;

    Ok(quote! {
        #crate_path::inventory::submit! {
            #crate_path::InventoryInstance {
                type_id: #type_id_lit,
                instance_id: #instance_id_lit,
                payload_fn: || ::serde_json::json!({
                    "id": #instance_id_lit,
                    #payload_tokens
                }),
            }
        }
    })
}
