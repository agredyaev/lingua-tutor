# 10 — Security and Privacy

Scope: threat model, authentication, validation, secrets, privacy.
Depends on: [01-architecture.md](01-architecture.md) trust boundaries.

## 1. Threat model

Primary risks: unauthorized access to learning history; forged review events; duplicate offline submissions; prompt injection via imported card text; malicious Markdown/HTML; accidental production DB replacement; excessive MCP permissions; incompatible/malicious pack assets; pack/core contract drift; secret exposure in WASM or logs.

## 2. Requirements

| Area | Requirement |
|---|---|
| Access | Cloudflare Access restricts browser app to owner email. Worker verifies the Access JWT (`Cf-Access-Jwt-Assertion`) on every request; identity from validated token only. |
| Writes | Authenticated identity + CSRF-safe design (Access-gated origin, JSON content type enforced, no cookie-only auth paths). |
| MCP | OAuth 2.1 + PKCE when enabled; scopes per [13-roadmap.md](13-roadmap.md) §3; disabled by default. |
| Idempotency | `client_operation_id` unique constraints; duplicates return original result. |
| Input | Imported tutor results and pack assets: schema-validated, size-bounded. Reject before parse when over limit. |
| Markup | Card and pack content: plain text or sanitized through allowlist. No script, no event attributes, no external resource loads. |
| Prompt injection | Imported lesson text is data, never instructions: rendered inert, size-bounded, and excluded from any tool/LLM instruction context. |
| Secrets | Never in WASM, repo, or Terraform state. Runtime secrets via `wrangler secret`. CI secrets in GitHub environments. |
| Logging | No full prompts/answers by default. Structured events only ([11-infrastructure.md](11-infrastructure.md) §6). |
| Exports | Explicit owner action only. |
| Infra | Terraform lifecycle protection on production D1; backup before destructive migrations ([11-infrastructure.md](11-infrastructure.md)). |
| Model access | ChatGPT never gets SQL, D1 credentials, arbitrary queries, full DB by default, or app secrets. Tools: narrow, validated inputs, bounded outputs. |

## 3. Privacy

Learning records leave the system only when user explicitly: uses ChatGPT/MCP; exports; enables a future opt-in integration. No advertising, no analytics trackers, no third-party behavioral SDKs. MVP stores no raw audio.
