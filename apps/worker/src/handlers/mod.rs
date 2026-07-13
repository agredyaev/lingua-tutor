//! `/api/v1` handlers (08-http-api.md). Thin: parse → fetch via
//! repositories → decide via `application`/domain → persist → respond.

pub mod cards;
pub mod export;
pub mod lessons;
pub mod profiles;
pub mod recommendations;
pub mod records;
pub mod review;
