//! Application operations grouped by domain. Transport parsing lives at the
//! JSON-RPC/MCP boundaries; file export is the only direct HTTP operation.

pub mod cards;
pub mod export;
pub mod lessons;
pub mod profiles;
pub mod recommendations;
pub mod records;
pub mod review;
