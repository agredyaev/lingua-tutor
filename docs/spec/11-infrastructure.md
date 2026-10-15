# 11 — Local Runtime, CI, Backup, Operations

## Runtime layout

- `lingua-tutor run`: materializes embedded UI assets, opens or creates `.lingua-tutor/lingua.sqlite3`, serves loopback UI/API, and connects the in-process MCP service to OpenAI Secure MCP Tunnel.
- The current working directory identifies the workspace.
- Repeated runs in the same directory reuse the same SQLite database.
- Tunnel ID and Runtime API key are session-only and never persisted.
- `lingua-tutor run --no-tunnel`: UI/API only.

## CI gate

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace`
4. scheduler golden/property tests
5. web build + size budget
6. self-contained CLI build
7. migration tests
8. local HTTP/MCP smoke tests
9. Playwright
10. dependency-direction checks and cargo-deny

## Installation

Install directly with `cargo install --path apps/local --bin lingua-tutor --locked`. No installer script, service manager, or separate tunnel installer is required.

## Backup and health

`lingua-tutor backup` uses SQLite online backup. `lingua-tutor check` validates the project database.

`GET /api/v1/health` reports runtime/schema health without private learning content or secrets.
