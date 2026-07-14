# ADR-0006 — MCP over Streamable HTTP with an embedded OAuth 2.1 server

Status: accepted (2026-07-14). Context: WBS 12.1 / M6, 13-roadmap.md §3, 10-security.md §2.

## Decision

The Worker itself serves the MCP surface and its OAuth 2.1 authorization
server. No separate service, no new crates.

1. **Transport**: hand-rolled JSON-RPC over `POST /mcp` (Streamable HTTP,
   stateless). `GET`/`DELETE /mcp` → 405 (spec-legal for stateless servers;
   if a client insists on an SSE stream, fallback is a no-op SSE response —
   not built until needed). Methods: `initialize`,
   `notifications/initialized`, `ping`, `tools/list`, `tools/call`.
2. **OAuth 2.1, embedded**: discovery metadata
   (`/.well-known/oauth-protected-resource`,
   `/.well-known/oauth-authorization-server`), RFC 7591 dynamic client
   registration (public clients, PKCE S256 only, redirect hosts allowlisted
   to ChatGPT domains), `/oauth/authorize` + `/oauth/token`.
3. **Tokens are opaque and hashed at rest** (SHA-256 in D1;
   `migrations/0002_oauth.sql`), not self-signed JWTs: revocable by row
   delete, no signing-key rotation story, one D1 point-read per call is
   nothing at single-user scale. Access TTL 1 h; refresh 30 d, rotated on use.
4. **Identity = Cloudflare Access**: `/oauth/authorize` stays behind the
   Access owner policy; the Worker re-verifies the Access JWT there. Every
   issued token acts as the owner, bounded by its scopes.
5. **Edge topology**: exact-path Access *bypass* applications (Terraform) for
   `/mcp`, `/oauth/token`, `/oauth/register` and the two well-known paths —
   never a wildcard that would unhook `/oauth/authorize`. The Worker enforces
   OAuth Bearer + per-tool scope on `/mcp` (defense-in-depth; edge bypass
   only removes the Access login wall).
6. **Tools reuse HTTP handlers** via an internal synthetic-request bridge —
   same validation, bounds, and error shapes; `complete_lesson` funnels into
   the same atomic `complete_from_result` path as the UI import
   ("no duplicate queries", roadmap §3).
7. **Gate**: `MCP_ENABLED` var; off → every MCP/OAuth path 404s. Default off;
   production enables only after the Terraform bypass apps exist.

## Rejected

- Self-signed JWT access tokens: revocation and key-rotation complexity for
  zero benefit at this scale.
- `oauth2`/`jsonwebtoken` crates: flows are small, deps carry wasm risk.
- Separate MCP worker: second deploy artifact, CORS, secret duplication.
