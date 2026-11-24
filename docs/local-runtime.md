# Local runtime

## One-command workspace

```text
chosen directory
└── .lingua-tutor/
    ├── lingua.sqlite3
    └── web/
```

Run from the directory whose learning state you want to use:

```sh
cd ~/spanish
lingua-tutor run
```

The current directory is the workspace identity. Re-running the command from the same directory reuses the same SQLite database.

## Runtime

```text
Browser
  │
  │ HTTP 127.0.0.1:8787
  ▼
lingua-tutor
  ├─ UI + /rpc + downloads
  ├─ SQLite: .lingua-tutor/lingua.sqlite3
  └─ in-process MCP
          │
          │ outbound Secure MCP Tunnel
          ▼
       OpenAI
          │
          ▼
       ChatGPT
```

There is no public application listener. The UI stays loopback-only.

The tunnel wire/policy implementation is adapted from Rivet. No `tunnel-client`, cloudflared, LaunchAgent, or second MCP process is required.

The built-in client implements the base OpenAI tunnel wire contract on `/v1/tunnels/...`: wire version `2026-08-25`, process-stable client instance ID, bounded polling, response deadlines anchored at HTTP response start, bounded retry jitter with `Retry-After` handling, opaque command shard-token echo, and forward-compatible unknown-command dropping. It intentionally does **not** advertise optional `X-Tunnel-Client-Capabilities: wrong-cluster-v1`; routing correction requires additional poll-placement state and response-header handling and must not be claimed until that behavior is implemented end to end. Services therefore use the documented legacy polling behavior for this client.

## Tunnel credentials

By default, `lingua-tutor run` starts the local HTTP listener and opens `/setup`. The setup page walks through tunnel registration and restricted runtime key creation. A successful initial poll advances to the dashboard. Tunnel errors leave the local UI available. Browser submission uses a same-origin JSON-RPC POST.

Use `lingua-tutor run --terminal-tunnel` to enter the credentials with terminal echo disabled:

```text
Tunnel ID:
Runtime API key:
```

Both values exist only in process memory. Lingua Tutor does not persist them in SQLite, files, environment configuration, or command-line arguments.

## Local-only mode

```sh
lingua-tutor run --no-tunnel
```

Then open `http://127.0.0.1:8787`.

## Android

Install the signed `lingua-tutor-v<version>-android.apk` (Android 9 or later, ARM64 or x86-64). Tap Lingua Tutor to open the setup screen inside the system WebView. No terminal, separate runtime, or external executable is required. Rust starts the same local HTTP server and SQLite database on a process-local background thread. The server binds a free loopback port and requires an app-private session cookie; Android networking uses Rustls with WebPKI roots.

The database is stored in the app-private files directory under `.lingua-tutor/lingua.sqlite3`. Installing an update signed with the same key retains the database. Uninstalling or clearing app data deletes it. Exports use Android's document picker. Existing desktop state is not imported automatically. The tunnel stays in memory and stops if Android kills the app process; reopen the app to reconnect.

### Build

Use JDK 17 or later, Android SDK platform 35, build-tools 35.0.0, and NDK 28.2.13676358. Build the web bundle first. Install Rust targets `aarch64-linux-android` and `x86_64-linux-android`.

Set `ANDROID_HOME`, `ANDROID_NDK_HOME`, `ANDROID_SIGNING_KEYSTORE`, `ANDROID_SIGNING_ALIAS`, `ANDROID_SIGNING_STORE_PASSWORD`, and `ANDROID_SIGNING_KEY_PASSWORD` in the build environment. Run:

```sh
bash scripts/build-android.sh
```

The script builds both ABIs, compiles the Android shell, aligns the APK, signs it, and checks the signature and package contents. The output is under `dist-android/`.

CI uses an ephemeral test signing key. Do not use a test-signed APK as a persistent installation: updates must have the same signing certificate. For release builds, configure repository secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEY_ALIAS`, `ANDROID_KEYSTORE_PASSWORD`, and `ANDROID_KEY_PASSWORD` with a persistent private signing key. Keep a secure offline backup of that key.

## Maintenance

```sh
lingua-tutor check
lingua-tutor backup
lingua-tutor where
```

Backups are written next to the active project database under the project state directory and validated by SQLite integrity/foreign-key checks.

## Install/update

Prebuilt release archives are the simplest installation path. For a source install, prepare the web bundle before the native release build:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
(cd apps/web && trunk build --release --cargo-profile wasm-release)
cargo install --path apps/local --bin lingua-tutor --locked
```

The release-profile build rejects a missing or incomplete generated web bundle instead of producing a native binary with a broken UI. No installer script or service wrapper is required.
