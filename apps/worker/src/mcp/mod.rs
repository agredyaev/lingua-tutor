//! MCP surface: Streamable HTTP transport + embedded OAuth 2.1 (WBS 12.1,
//! ADR-0006). Routed from `fetch` BEFORE the Access gate; every path here
//! owns its authentication per the auth matrix in ADR-0006.

use worker::{Env, Request, Response, Result};

/// Exact-path classification for the MCP/OAuth surface. Anything not listed
/// falls through to the normal Access-gated app — no prefix or substring
/// matching (security-critical branch, ADR-0006 §5).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum McpPath {
    /// `POST /mcp` JSON-RPC endpoint (OAuth Bearer).
    Mcp,
    /// `GET|POST /oauth/authorize` — stays behind Cloudflare Access.
    Authorize,
    /// `POST /oauth/token` — public, grant validation inside.
    Token,
    /// `POST /oauth/register` — public, RFC 7591 subset.
    Register,
    /// `GET /.well-known/oauth-protected-resource` — public metadata.
    ProtectedResourceMetadata,
    /// `GET /.well-known/oauth-authorization-server` — public metadata.
    AuthServerMetadata,
    /// Not part of the MCP surface.
    NotMcp,
}

/// Classify a URL path (already percent-decoded and normalized by the
/// `Request` URL parser).
#[must_use]
pub fn classify(path: &str) -> McpPath {
    match path {
        "/mcp" => McpPath::Mcp,
        "/oauth/authorize" => McpPath::Authorize,
        "/oauth/token" => McpPath::Token,
        "/oauth/register" => McpPath::Register,
        "/.well-known/oauth-protected-resource" => McpPath::ProtectedResourceMetadata,
        "/.well-known/oauth-authorization-server" => McpPath::AuthServerMetadata,
        _ => McpPath::NotMcp,
    }
}

/// True when the MCP feature is switched on for this deployment.
fn enabled(env: &Env) -> bool {
    env.var("MCP_ENABLED")
        .is_ok_and(|v| v.to_string() == "true")
}

/// Route an MCP-surface request. Callers must have classified `path` as a
/// non-[`McpPath::NotMcp`] variant.
///
/// # Errors
/// Propagates `worker` runtime failures; protocol errors become responses.
pub async fn route(path: McpPath, req: Request, env: &Env) -> Result<Response> {
    if !enabled(env) {
        // Feature off: indistinguishable from a route that never existed.
        return Response::error("not found", 404);
    }
    let _ = (path, req);
    // Endpoints land in subsequent tasks (oauth.rs, transport.rs, tools.rs).
    Response::error("not found", 404)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{McpPath, classify};

    #[test]
    fn exact_paths_classify() {
        assert_eq!(classify("/mcp"), McpPath::Mcp);
        assert_eq!(classify("/oauth/authorize"), McpPath::Authorize);
        assert_eq!(classify("/oauth/token"), McpPath::Token);
        assert_eq!(classify("/oauth/register"), McpPath::Register);
        assert_eq!(
            classify("/.well-known/oauth-protected-resource"),
            McpPath::ProtectedResourceMetadata
        );
        assert_eq!(
            classify("/.well-known/oauth-authorization-server"),
            McpPath::AuthServerMetadata
        );
    }

    #[test]
    fn lookalikes_and_traversals_do_not_classify() {
        for path in [
            "/mcpx",
            "/mcp/",
            "/mcp/../api/v1/health",
            "/oauthfoo",
            "/oauth/authorize/extra",
            "/oauth/",
            "/OAUTH/TOKEN",
            "/.well-known/oauth-protected-resource/x",
            "/api/v1/health",
            "/",
            "",
        ] {
            assert_eq!(classify(path), McpPath::NotMcp, "path: {path}");
        }
    }
}
