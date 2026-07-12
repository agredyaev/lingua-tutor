# 14 — Traceability

Scope: proves every `docs/spec.md` v2.0 section is covered; lists external assumptions to revalidate before version pinning.

## 1. Section map

| spec.md § | Topic | Spec file |
|---|---|---|
| 1 | Executive summary | 00 §1, §3 |
| 2 | Product objectives, outcomes, indicators | 00 §1; 03 §9 |
| 3.1–3.2 | Tutor-managed content; phrase-first | 07 §1; 03 §5 |
| 3.3 | Simplicity constraints | 00 §3 |
| 3.4 | Rust-first | 00 §3; 09 §1 |
| 3.5 | App-owned state | 00 §3; 04 §3 |
| 3.6 | No model DB access | 10 §2 |
| 3.7 | No hidden AI dependency | 00 §3; 13 §2 |
| 3.8 | Free tier | 00 §3; 11 §7 |
| 3.9 | No raw audio | 00 §3; 04 (`listening_attempts`); 10 §3 |
| 3.10 | Language-agnostic core | 00 §5; 06 §1; 12 §10 |
| 3.11 | Packs are plugins | 06 §1–§4 |
| 3.12 | Subsystem boundaries | 01 §1 |
| 4 | Scope | 00 §4 |
| 5 | System context, trust boundaries | 01 §1–§2 |
| 6.1–6.9 | Architecture decisions | 01 AD-1…AD-9 |
| 7.1 | Naming | 00 §5 |
| 7.2–7.4 | Core/review/pack responsibilities | 06 §1; 05; 02 §2 |
| 7.5–7.6 | Pack contract, manifest | 06 §2–§3 |
| 7.7 | Plugin lifecycle | 06 §4–§5 |
| 7.8 | Spanish pack | 06 §7 |
| 7.9 | Plugin non-goals | 06 §9 |
| 8.1–8.9 | Tutor card generation | 07 §1–§4 |
| 9 | Card types | 07 §5 |
| 10.1–10.9 | Review engine | 05 §3–§10 |
| 10.10 | Parameter optimization | 05 §11; 13 M8 |
| 11 | Adaptive rules | 07 §7; 02 (`adaptation`) |
| 12.1 | Profiles | 03 §2–§3 |
| 12.2 | Dashboard | 08 §3; 09 §6 |
| 12.3 | Lesson completion | 07 §2; 08 §4 |
| 12.4 | Mastery | 03 §6 |
| 12.5 | Review session | 08 §6; 09 §4 |
| 12.6 | Export | 08 §7 |
| 13 | User workflows | 13 §2 (no-MCP); 13 §3 (MCP); 09 §4 (review); 07 §6 (redesign) |
| 14.1–14.17 | Data model | 04 §2–§4 |
| 15 | HTTP API | 08 |
| 16 | MCP | 13 §3 |
| 17 | Frontend | 09 |
| 18 | Security + privacy | 10 |
| 19 | Repository layout | 02 §1 |
| 20 | Technology baseline + dep policy | 02 §4–§5 |
| 21 | Terraform/Cloudflare | 11 §1–§2 |
| 22 | CI/CD | 11 §3–§4 |
| 23 | Testing strategy | 12 |
| 24 | Observability | 11 §6 |
| 25 | Backup + recovery | 11 §5 |
| 26 | Performance + capacity | 12 §8; 09 §9 |
| 27 | Delivery plan | 13 §1 |
| 28 | Definition of done | 00 §6 |
| 29 | Risks | 13 §4 |
| 30 | Initial implementation instructions | 13 M0–M1; 12 §9–§10; 02 §5 |
| 31 | External assumptions | 14 §2 |
| 32 | Final architecture decision | 00 §1; 01 |
| 33 | Migration/naming map from v1.1 | 00 §5 (implementation starts with v2.0 names; no aliases) |

## 2. External assumptions — revalidate before pinning versions

1. FSRS: open algorithm on difficulty/stability/retrievability.
2. Rust FSRS implementations exist (scheduler-oriented and optimizer-oriented).
3. Desired retention ≈ 0.90 reasonable default; higher = more workload.
4. Rating semantics: forgetting = `Again`, never passing `Hard`.
5. Cloudflare Workers run Rust via supported toolchain (`workers-rs`).
6. D1 = managed SQLite-compatible storage, fits this workload.
7. Terraform manages Cloudflare resources; Wrangler preferred for deploy + migrations.
8. Remote MCP depends on current ChatGPT account availability + OAuth-compatible deployment.
9. Build-time Rust plugin registration sufficient; no runtime plugin mechanism needed.
10. Pack compatibility enforced by app-owned integer contract version, independent of package semver.

Also revalidate current docs for: Leptos (APIs in 09 §2), Trunk, workers-rs, D1 limits, Terraform Cloudflare Provider v5.
