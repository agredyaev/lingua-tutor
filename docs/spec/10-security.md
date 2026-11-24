# 10 — Security and Privacy

Scope: active local threat model.

## 1. Threat model

Primary risks are malformed/imported content, duplicate or forged local writes, prompt injection through stored lesson/card text, unsafe markup, accidental database loss, excessive MCP permissions, and secret leakage.

## 2. Requirements

| Area | Requirement |
|---|---|
| HTTP exposure | Bind only to `127.0.0.1`; reject unexpected Host or Origin headers. Android also checks the random session cookie on every request. Desktop has no equivalent cookie requirement. |
| Writes | Validate typed inputs; preserve idempotency with `client_operation_id`. |
| MCP | Expose narrow tools only; no raw SQL, arbitrary code, arbitrary URLs, or unbounded reads. |
| Tunnel | Use the OpenAI Secure MCP Tunnel protocol. Accept credentials through same-origin loopback setup or hidden terminal input. Keep them in process memory for the session. |
| Input | Validate tutor results against the schema and operation limits before writes. |
| Markup | Learning content is plain text or sanitized; no executable markup. |
| Prompt injection | Stored/imported learner content is data, never tool or system instructions. |
| Secrets | Do not persist tunnel credentials in the repository, browser storage, database exports or logs. Store release signing credentials in GitHub Actions Secrets and a private backup. |
| Logging | Do not log full private prompts/answers by default. |
| Backups | Create a verified backup at startup when due. Android backups are app-private and deleted on uninstall; system backup is disabled. Restore behavior is covered by SQLite tests. |
| Model access | ChatGPT receives only data exposed by explicit bounded MCP tools. |

## 3. Privacy

Learning records remain local except when the user explicitly sends/receives data through ChatGPT MCP or exports data. No advertising, behavioral analytics, or third-party tracking SDKs.
