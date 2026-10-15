//! Native Lingua Tutor runtime and local SQLite-backed API.

mod runtime;
pub use runtime::*;

pub mod api;
pub mod assets;
pub mod handlers;
pub mod mcp;
pub mod mcp_tools;
pub mod repositories;
pub mod server;
pub mod storage;
pub mod tunnel;
mod tunnel_policy;
mod tunnel_wire;

/// Build the immutable production language-pack registry at process startup.
pub fn built_in_language_packs() -> std::result::Result<
    language_pack_registry::LanguagePackRegistry,
    language_pack_registry::RegistryError,
> {
    language_pack_registry::LanguagePackRegistry::new([language_pack_es_es::register()])
}
