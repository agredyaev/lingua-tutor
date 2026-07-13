# M5 security review (WBS 10.8) — vs 10-security.md §2

Date: 2026-07-13. Reviewer: owner + assistant. Status per requirement:

| Area | Status | Evidence |
|---|---|---|
| Access (edge) | ✅ configured | Terraform: Access app on the workers.dev host + owner-email policy; `auto_redirect_to_identity` |
| Access (Worker JWT) | ✅ implemented | `auth.rs`: RS256 vs team JWKS, kid, aud, exp, owner email; enforced by `AUTH_ENFORCED=true` in the production env only |
| CSRF-safe writes | ✅ | Access-gated origin; JSON bodies parsed strictly; no cookie-only auth path in the Worker (identity = validated JWT header) |
| MCP | ✅ n/a | disabled — not built (post-MVP, 13-roadmap §3) |
| Idempotency | ✅ | UNIQUE `client_operation_id` on `review_events`, `card_mutations`, lessons `idempotency_key`; duplicates return the original result (smoke steps 8, 13) |
| Input bounds | ✅ | 256 KB body cap before parse; tutor-contract bounds (collections + text); `deny_unknown_fields`; fuzz target 7.4M execs clean |
| Markup | ✅ | All card/pack content rendered as Leptos text nodes (auto-escaped); no `inner_html` anywhere in `apps/web` |
| Prompt injection | ✅ | Imported lesson text stored as data; tutor context includes only structured records + pack instructions, never imported free text as instructions |
| Secrets | ✅ | No secrets in repo/WASM/Terraform state (token via env; state has resource IDs only — verified); CI secrets in GitHub environment |
| Logging | ✅ | No prompt/answer bodies logged; Worker errors are typed codes |
| Exports | ✅ | GET endpoints, explicit user action from /export |
| Infra | ✅ | `prevent_destroy` on prod D1; CD backs up before migrations; drift gate blocks deploys |
| Model access | ✅ | ChatGPT sees only tutor-context export + pasted results; no SQL/credentials/tools |

Residual risks (accepted, tracked):

1. Import validation token is unsigned (FNV + expiry) — binds apply to the validated payload; forgery is moot behind Access (single owner). Revisit only if multi-user ever happens (it won't, v1).
2. API token for CI expires 2026-07-21 — owner to mint a long-lived scoped token for GitHub secrets before then.
3. Access dashboard recovery: owner email is the single identity — keep Cloudflare account recovery healthy.
