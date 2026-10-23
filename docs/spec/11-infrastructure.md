# 11 — Local Runtime, CI, Backup, Operations

## Runtime layout

- `lingua-tutor run`: materializes embedded UI assets, opens or creates `.lingua-tutor/lingua.sqlite3`, serves loopback static assets + `POST /rpc` + export downloads, and connects the in-process MCP service to OpenAI Secure MCP Tunnel.
- The current working directory identifies the workspace.
- Repeated runs in the same directory reuse the same SQLite database.
- Tunnel ID and Runtime API key are hidden-input, session-only values and are never persisted.
- `lingua-tutor run --no-tunnel`: browser UI/RPC only.
- Runtime-owned migrations live under `apps/local/migrations/`.

## CI gate

1. user-documentation release contract
2. `cargo fmt --check`
3. strict workspace clippy
4. workspace tests
5. web build + WASM size budget
6. cargo-deny
7. native macOS/Linux build and smoke
8. Linux x64/aarch64 cross-build verification
9. Playwright e2e
10. staged release assets + SHA256 verification

## Installation and release distribution

The supported end-user installation path is a verified native binary from GitHub Releases. Each platform ZIP contains exactly one executable named `lingua-tutor`.

Published targets:

- macOS Apple Silicon: `lingua-tutor-vX.Y.Z-darwin-arm64.zip`
- Linux x86_64: `lingua-tutor-vX.Y.Z-linux-x64.zip`
- Linux ARM64: `lingua-tutor-vX.Y.Z-linux-aarch64.zip`
- `SHA256SUMS` verifies the downloaded archives against the checksums published with the release; it is an integrity check, not a code-signing identity proof.

Rust/Cargo, Trunk, Node.js, a service manager, installer wrapper, separate web bundle, and separate tunnel executable are not runtime prerequisites.

Source builds are for contributors and CI.
Release-user installation uses published binaries.
User-facing commands live in [../installation.md](../installation.md).

## Backup and health

`lingua-tutor backup` uses SQLite online backup. `lingua-tutor check` validates the project database.

Runtime health is the `health` JSON-RPC operation. It reports runtime/schema health without private learning content or secrets.
