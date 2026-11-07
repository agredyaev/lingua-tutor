//! Native Lingua Tutor runtime and local SQLite-backed API.

mod runtime;
pub use runtime::*;

pub(crate) mod api;
pub mod assets;
#[cfg(fuzzing)]
pub mod fuzzing;
pub(crate) mod handlers;
pub(crate) mod mcp;
pub(crate) mod mcp_tools;
pub(crate) mod operations;
pub mod server;
pub(crate) mod sqlite;
pub mod storage;
pub mod tunnel;
mod tunnel_policy;
mod tunnel_wire;
