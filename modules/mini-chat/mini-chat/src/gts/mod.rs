//! Mini-chat's link-time GTS content.
//!
//! Everything declared here reaches `types-registry` automatically through
//! the process-wide `modkit-gts` inventory — no registration code in
//! [`crate::module::MiniChatModule::init`] is needed for entries below.
//!
//! One file per content kind keeps this directory navigable as more GTS
//! content accrues (permissions today; future candidates — audit-event
//! topics, role templates, …). Avoid a grab-bag `gts.rs` at crate root.

mod permissions;
