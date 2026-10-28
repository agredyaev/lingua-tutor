//! Native Lingua Tutor runtime and local SQLite-backed API.

mod runtime;
pub use runtime::*;

pub mod api;
pub mod assets;
#[cfg(feature = "fuzzing")]
pub mod fuzzing;
pub mod handlers;
pub mod mcp;
pub mod mcp_tools;
pub(crate) mod operations;
pub mod repositories;
pub mod server;
pub mod storage;
pub mod tunnel;
mod tunnel_policy;
mod tunnel_wire;

/// Concrete language pack composition used by the native runtime.
pub type BuiltInPackRegistry =
    language_pack_registry::LanguagePackRegistry<language_pack_es_es::EsEsPack>;

/// Build the immutable production language-pack registry at process startup.
pub fn built_in_language_packs()
-> std::result::Result<BuiltInPackRegistry, language_pack_registry::RegistryError> {
    language_pack_registry::build_registry([language_pack_es_es::register()])
}
