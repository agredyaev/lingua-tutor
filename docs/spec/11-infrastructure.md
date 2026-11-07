# 11 — Local Runtime, CI, Backup, Operations

## Runtime layout

- `lingua-tutor run`: materializes embedded UI assets, opens or creates `.lingua-tutor/lingua.sqlite3`, serves loopback static assets + `POST /rpc` + export downloads, and connects the in-process MCP service to OpenAI Secure MCP Tunnel.
- The current working directory identifies the workspace.
- Repeated runs in the same directory reuse the same SQLite database.
- Tunnel ID and Runtime API key are hidden-input, session-only values and are never persisted.
- `lingua-tutor run --no-tunnel`: browser UI/RPC only.
- Tunnel mode uses the OS-provided curl executable. macOS uses `/usr/bin/curl`, Linux accepts `/usr/bin/curl` or `/bin/curl`, and Windows uses `%SystemRoot%\\System32\\curl.exe`. Hidden credential input uses the controlling TTY plus `stty` on Unix and Windows PowerShell on Windows.
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

Use a verified release binary, or for a source install build `apps/web` with Trunk before running `cargo install --path apps/local --bin lingua-tutor --locked`. Release-profile native builds reject a missing/incomplete generated web bundle. No service manager, installer wrapper, or separate tunnel executable is required.

## Backup and health

`lingua-tutor backup` uses SQLite online backup. `lingua-tutor check` validates the project database.

Runtime health is the `health` JSON-RPC operation. It reports runtime/schema health without private learning content or secrets.
