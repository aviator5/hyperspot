//! GTS schema definitions for `AuthZ` resolver plugins.

use modkit::gts::PluginV1;
use modkit_gts::gts_schema;

/// GTS type definition for `AuthZ` resolver plugin instances.
///
/// # Instance ID Format
///
/// ```text
/// gts.cf.modkit.plugins.plugin.v1~<vendor>.<package>.authz_resolver.plugin.v1~
/// ```
#[gts_schema(
    base = PluginV1,
    schema_id = "gts.cf.modkit.plugins.plugin.v1~cf.core.authz_resolver.plugin.v1~",
    description = "AuthZ Resolver plugin specification",
    properties = ""
)]
pub struct AuthZResolverPluginSpecV1;
