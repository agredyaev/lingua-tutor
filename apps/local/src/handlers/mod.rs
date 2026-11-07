//! Application operations grouped by domain. Transport parsing lives at the
//! JSON-RPC/MCP boundaries; file export is the only direct HTTP operation.

pub(crate) mod cards;
pub(crate) mod export;
pub(crate) mod lessons;
pub(crate) mod profiles;
pub(crate) mod records;
pub(crate) mod review;
