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

Every `lingua-tutor run` prompts with terminal echo disabled:

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

## Maintenance

```sh
lingua-tutor check
lingua-tutor backup
lingua-tutor where
```

Backups are written next to the active project database under the project state directory and validated by SQLite integrity/foreign-key checks.

## Install/update

```sh
cargo install --path apps/local --bin lingua-tutor --locked
```

The CLI is installed directly by Cargo. No installer script or service wrapper is required.
