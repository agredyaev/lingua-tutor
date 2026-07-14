# ChatGPT MCP connector — owner checklist (WBS 12.1, M6)

Prereq: deploy green with MCP smoke steps; Terraform bypass apps applied.

## Connect

- [ ] ChatGPT → Settings → Connectors → enable developer mode (Plus/Pro).
- [ ] Add custom connector: `https://lingua-tutor-api.aleksey-gredyaev-dev.workers.dev/mcp`.
- [ ] ChatGPT registers via DCR, then opens `/oauth/authorize` → Cloudflare
      Access login (One-Time PIN, owner email) → consent page → Approve.
- [ ] Connector shows the tool list (15 tools).

## Validate (desktop)

- [ ] Read: "What should my next Spanish lesson cover?" → ChatGPT calls
      `get_tutor_context` / `get_next_lesson`; answer reflects real state.
- [ ] Lesson loop: run a short lesson; at the end ChatGPT calls
      `complete_lesson` (write confirmation prompt appears) → verify in the
      PWA: lesson number advanced, cards/errors recorded.
- [ ] Retry safety: ask it to submit the same lesson again → duplicate noop.
- [ ] Scope guard: connector token without write scopes cannot call
      `start_lesson` (insufficient_scope).

## Validate (mobile — M6 requirement)

- [ ] ChatGPT iOS app: same connector available; run one read tool.
- [ ] Latency acceptable for lesson flow.

## Kill switches

- [ ] Revoke: delete rows from `oauth_tokens` (or drop the client row) →
      connector must re-authenticate through Access.
- [ ] Feature off: set `MCP_ENABLED` to `false` in wrangler.jsonc production
      vars and redeploy → all MCP/OAuth paths 404.
