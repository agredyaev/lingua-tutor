# Lingua Tutor

Local-first adaptive language tutor with one SQLite source of truth.

- PWA + JSON-RPC `/rpc` on `127.0.0.1:8787`;
- project-local SQLite;
- 15 MCP tools over the same native processing logic;
- built-in OpenAI Secure MCP Tunnel protocol adapted from Rivet;
- tunnel ID and Runtime API key are requested with hidden terminal input on every run and are never written to disk.

## Install

From source, build the generated web bundle first:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
(cd apps/web && trunk build --release --cargo-profile wasm-release)
cargo install --path apps/local --bin lingua-tutor --locked
```

Release-profile native builds fail closed when the generated web bundle is missing or incomplete. Cargo installs `lingua-tutor` into its normal binary directory (typically `~/.cargo/bin`).

Prebuilt release archives are published for Linux x64, Linux arm64, macOS arm64, and Windows x64.

## Update

Project data is stored separately under `<project>/.lingua-tutor/`, so replacing the executable does not replace or reset the learner database.

If installed from source, update the checkout, rebuild the generated web bundle, and reinstall:

```sh
git pull
(cd apps/web && trunk build --release --cargo-profile wasm-release)
cargo install --path apps/local --bin lingua-tutor --locked --force
```

If using a prebuilt release, download the archive for the new version and replace only the executable:

- Linux/macOS: replace `lingua-tutor`.
- Windows x64: replace `lingua-tutor.exe`.

Before an upgrade, `lingua-tutor backup` can be used to create an explicit database backup.

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
