# Lingua Tutor

Local language-learning runtime with one SQLite database.

- PWA + JSON-RPC `/rpc` on `127.0.0.1:8787`;
- project-local SQLite;
- 15 MCP tools over the same application logic;
- built-in OpenAI Secure MCP Tunnel protocol adapted from Rivet;
- tunnel ID and Runtime API key are requested with hidden terminal input on every run and are never written to disk.

## Quick start

Lingua Tutor is distributed as a self-contained native executable. End users do **not** need Rust, Cargo, Node.js, Trunk, or a separate tunnel client.

1. Open the [latest GitHub release](https://github.com/agredyaev/lingua-tutor/releases/latest).
2. Download `SHA256SUMS` and the archive for your platform:
   - macOS Apple Silicon: `lingua-tutor-vX.Y.Z-darwin-arm64.zip`
   - Linux x86_64: `lingua-tutor-vX.Y.Z-linux-x64.zip`
   - Linux ARM64: `lingua-tutor-vX.Y.Z-linux-aarch64.zip`
3. Verify the SHA-256 checksum, unzip the archive, and install the single `lingua-tutor` executable.

Platform commands, checksum verification, macOS Gatekeeper handling, updates, uninstall, and troubleshooting: [docs/installation.md](docs/installation.md).

## Run

Choose a directory for one learning workspace:

```sh
mkdir -p ~/spanish
cd ~/spanish
lingua-tutor run
```

The first run creates runtime state under the current directory:

```text
<your chosen directory>/
└── .lingua-tutor/
    ├── lingua.sqlite3
    └── web/
```

The release archive contains one executable. The executable embeds the web application and runtime resources, then materializes the web assets into the workspace state directory when it runs.

Running `lingua-tutor run` again from the same directory reuses the same database.

The CLI prompts with terminal echo disabled for:

```text
Tunnel ID:
Runtime API key:
```

Input is not echoed. Credentials live only in process memory for that run.

For UI-only local use:

```sh
lingua-tutor run --no-tunnel
```

Open `http://127.0.0.1:8787`.

Maintenance commands:

```sh
lingua-tutor check
lingua-tutor backup
lingua-tutor where
```

See [docs/local-runtime.md](docs/local-runtime.md) for runtime behavior and workspace semantics.

## Development

Development and CI use Cargo. Release installation does not.

Repository acceptance checks include:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace
cargo deny --locked check
cd apps/web && trunk build --release --cargo-profile wasm-release
```

See [docs/spec/12-testing.md](docs/spec/12-testing.md) for verification commands and acceptance criteria.
