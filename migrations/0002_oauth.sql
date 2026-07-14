-- OAuth 2.1 storage for the MCP surface (13-roadmap.md §3, 10-security.md §2,
-- ADR-0006). Single-owner: every token acts as the owner; scopes bound what
-- a token may call. Secrets never stored raw — only SHA-256 hashes.

CREATE TABLE oauth_clients (
    client_id          TEXT PRIMARY KEY,
    client_name        TEXT NOT NULL,
    redirect_uris_json TEXT NOT NULL, -- JSON array; https-only, host-allowlisted at registration
    created_at         TEXT NOT NULL
);

CREATE TABLE oauth_codes (
    code_hash      TEXT PRIMARY KEY, -- sha256 hex of the authorization code
    client_id      TEXT NOT NULL REFERENCES oauth_clients(client_id),
    redirect_uri   TEXT NOT NULL,
    scope          TEXT NOT NULL,    -- space-joined subset of the six scopes
    code_challenge TEXT NOT NULL,    -- PKCE S256 only
    expires_at     TEXT NOT NULL,
    used           INTEGER NOT NULL DEFAULT 0 CHECK (used IN (0, 1))
);

CREATE TABLE oauth_tokens (
    token_hash TEXT PRIMARY KEY,     -- sha256 hex of the bearer token
    kind       TEXT NOT NULL CHECK (kind IN ('access', 'refresh')),
    client_id  TEXT NOT NULL REFERENCES oauth_clients(client_id),
    scope      TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked    INTEGER NOT NULL DEFAULT 0 CHECK (revoked IN (0, 1)),
    created_at TEXT NOT NULL
);

CREATE INDEX ix_oauth_tokens_client ON oauth_tokens(client_id, kind);
