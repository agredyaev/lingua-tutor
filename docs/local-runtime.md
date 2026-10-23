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

## Install and update

End users install the self-contained executable from [GitHub Releases](https://github.com/agredyaev/lingua-tutor/releases/latest). The release archive contains exactly one file named `lingua-tutor`; Rust and Cargo are not runtime prerequisites.

Use [installation.md](installation.md) for supported targets, checksum verification, macOS Gatekeeper handling, updates, uninstall, and troubleshooting.

No service manager, installer wrapper, separate web bundle, or separate tunnel executable is required.
