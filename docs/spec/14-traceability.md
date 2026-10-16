# 14 — Traceability

This file maps active architectural requirements to their canonical documentation.

| Requirement | Canonical source |
|---|---|
| Local-first topology | 01-architecture.md |
| Workspace/dependency direction | 02-workspace.md |
| Domain model | 03-domain-model.md |
| SQLite schema and migrations | 04-database.md |
| Review scheduling | 05-review-engine.md |
| Language-pack contract | 06-language-packs.md |
| Tutor import/export contract | 07-tutor-contract.md |
| Local browser RPC/download boundary | 08-local-rpc.md |
| PWA behavior | 09-frontend.md |
| Security/privacy | 10-security.md |
| Local operations/backup/CI | 11-infrastructure.md |
| Testing | 12-testing.md |
| Roadmap and risks | 13-roadmap.md |
| Rust engineering rules | 16-rust-guidelines.md |
| Documentation rules | 17-documentation.md |

External/version-sensitive assumptions that must be revalidated when changed:

1. current Rust/Leptos/Trunk compatibility;
2. SQLite/rusqlite behavior used by migrations and online backup;
3. current OpenAI Secure MCP Tunnel wire contract and in-process MCP JSON-RPC behavior;
4. current OpenAI Secure MCP Tunnel wire/API contract and Rivet tunnel policy/wire behavior;
5. scheduler crate compatibility and deterministic behavior.

No remote hosting provider is part of the application architecture.
