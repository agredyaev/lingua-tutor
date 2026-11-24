# Lingua Tutor

Local-first adaptive language tutor with one SQLite source of truth.

- PWA + JSON-RPC `/rpc` on `127.0.0.1:8787`;
- project-local SQLite;
- 15 MCP tools over the same native processing logic;
- built-in OpenAI Secure MCP Tunnel protocol adapted from Rivet;
- browser setup for the tunnel ID and Runtime API key. Both stay in memory for the session.

## Install

From source, build the generated web bundle first:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
(cd apps/web && trunk build --release --cargo-profile wasm-release)
cargo install --path apps/local --bin lingua-tutor --locked
```

Release-profile native builds fail closed when the generated web bundle is missing or incomplete. Cargo installs `lingua-tutor` into its normal binary directory (typically `~/.cargo/bin`).

Prebuilt release archives are published for Linux x64, Linux arm64, macOS arm64, and Windows x64. Android uses a signed APK (ARM64 and x86-64).

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

Android updates install over the existing app when signed with the same key; uninstalling deletes app-private learning state.

Before a desktop upgrade, `lingua-tutor backup` can be used to create an explicit database backup.

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

The runtime starts the web server and opens the tunnel setup in your browser. If no browser opens, use the URL printed by the CLI (normally `http://127.0.0.1:8787/setup`). Follow the instructions on that page to create the tunnel and a restricted runtime key. Choose **Continue locally** to open the dashboard without connecting a tunnel.

Use `lingua-tutor run --terminal-tunnel` for hidden terminal input.

On Android, [install the APK](docs/local-runtime.md#android) and tap Lingua Tutor. The native runtime and WebView start together.

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
