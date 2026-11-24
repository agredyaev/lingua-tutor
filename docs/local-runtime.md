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

## Android home-screen shortcut

On an ARM64 tablet, use Termux, Debian via PRoot-Distro, and the Linux ARM64 release archive. Install [Termux:Widget](https://github.com/termux/termux-widget) from the same source as Termux. Run `termux-setup-storage` and grant access to Downloads. Extract the executable to `~/lingua/lingua-tutor` and `termux-launcher.sh` to Downloads, then set up the shortcut once in Termux.

```sh
pkg install proot-distro
proot-distro install debian
proot-distro login debian -- sh -c 'apt-get update && apt-get install -y curl ca-certificates'
install -Dm700 ~/storage/downloads/termux-launcher.sh ~/.shortcuts/Lingua\ Tutor
```

Reuse the existing Debian installation if you already have one. On Android 10 or later, allow Termux to appear over other apps in Android settings so the widget can open its foreground session. Add **Termux:Widget** to the home screen and select **Lingua Tutor**. Tapping the shortcut starts the local server and opens the setup page. Later taps reopen the running session.

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
