//! Native Lingua Tutor runtime and local SQLite-backed API.

mod runtime;
pub use runtime::*;

pub mod api;
pub mod assets;
#[cfg(fuzzing)]
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
