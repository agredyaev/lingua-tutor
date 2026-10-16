# 10 — Security and Privacy

Scope: active local threat model.

## 1. Threat model

Primary risks are malformed/imported content, duplicate or forged local writes, prompt injection through stored lesson/card text, unsafe markup, accidental database loss, excessive MCP permissions, and secret leakage.

## 2. Requirements

| Area | Requirement |
|---|---|
| HTTP exposure | Bind only to `127.0.0.1`; reject non-loopback host/origin patterns. |
| Writes | Validate typed inputs; preserve idempotency with `client_operation_id`. |
| MCP | Expose narrow tools only; no raw SQL, arbitrary code, arbitrary URLs, or unbounded reads. |
| Tunnel | Use the OpenAI Secure MCP Tunnel protocol; prompt for tunnel ID/API key with hidden terminal input per run and keep both in process memory only. |
| Input | Tutor results and pack assets are schema-validated and size-bounded before use. |
| Markup | Learning content is plain text or sanitized; no executable markup. |
| Prompt injection | Stored/imported learner content is data, never tool or system instructions. |
| Secrets | Never store secrets in the repository, PWA, database exports, or logs. |
| Logging | Do not log full private prompts/answers by default. |
| Backups | Local SQLite backups are created automatically and restore-tested. |
| Model access | ChatGPT receives only data exposed by explicit bounded MCP tools. |

## 3. Privacy

Learning records remain local except when the user explicitly sends/receives data through ChatGPT MCP or exports data. No advertising, behavioral analytics, or third-party tracking SDKs.
