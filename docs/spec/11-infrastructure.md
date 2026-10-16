# 11 — Local Runtime, CI, Backup, Operations

## Runtime layout

- `lingua-tutor run`: materializes embedded UI assets, opens or creates `.lingua-tutor/lingua.sqlite3`, serves loopback static assets + `POST /rpc` + export downloads, and connects the in-process MCP service to OpenAI Secure MCP Tunnel.
- The current working directory identifies the workspace.
- Repeated runs in the same directory reuse the same SQLite database.
- Tunnel ID and Runtime API key are hidden-input, session-only values and are never persisted.
- `lingua-tutor run --no-tunnel`: browser UI/RPC only.
- Runtime-owned migrations live under `apps/local/migrations/`.

## CI gate

1. `cargo fmt --check`
2. strict workspace clippy
3. workspace tests
4. web build + WASM size budget
5. cargo-deny
6. native macOS/Linux build and smoke
7. Linux x64/aarch64 cross-build verification
8. Playwright e2e
9. staged release assets + SHA256 verification

## Installation

Install from source with `cargo install --path apps/local --bin lingua-tutor --locked`, or use a verified release binary. No service manager, installer wrapper, or separate tunnel executable is required.

## Backup and health

`lingua-tutor backup` uses SQLite online backup. `lingua-tutor check` validates the project database.

Runtime health is the `health` JSON-RPC operation. It reports runtime/schema health without private learning content or secrets.
