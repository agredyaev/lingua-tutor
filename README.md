# Lingua Tutor

Local-first adaptive language tutor with one SQLite source of truth.

- PWA + JSON-RPC `/rpc` on `127.0.0.1:8787`;
- project-local SQLite;
- 15 MCP tools over the same application logic;
- built-in OpenAI Secure MCP Tunnel protocol adapted from Rivet;
- tunnel ID and Runtime API key are requested with hidden terminal input on every run and are never written to disk.

## Install

```sh
cargo install --path apps/local --bin lingua-tutor --locked
```

Cargo installs `lingua-tutor` into its normal binary directory (typically `~/.cargo/bin`).

## Run

Choose a directory for one learning workspace:

```sh
mkdir -p ~/spanish
cd ~/spanish
lingua-tutor run
```

The first run creates:

```text
<your chosen directory>/
└── .lingua-tutor/
    ├── lingua.sqlite3
    └── web/
```

More precisely, state is always under `<current-directory>/.lingua-tutor/`. Running `lingua-tutor run` again from the same directory reuses the same database.

The CLI securely prompts for:

```text
Tunnel ID:
Runtime API key:
```

Input is not echoed. Credentials live only in process memory for that run.

For UI-only local testing:

```sh
lingua-tutor run --no-tunnel
```

Open:

```text
http://127.0.0.1:8787
```

Useful commands:

```sh
lingua-tutor check
lingua-tutor backup
lingua-tutor where
```

## Verify

```sh
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --check
cd apps/web && trunk build --release --cargo-profile wasm-release
```

See [docs/local-runtime.md](docs/local-runtime.md).
