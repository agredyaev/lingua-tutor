# Lingua Tutor

Language-agnostic personal language-learning platform. One learner. Tutor lessons run in ChatGPT; this app stores state, schedules reviews, adapts. Spec: [docs/spec/](docs/spec/00-overview.md), baseline [docs/spec.md](docs/spec.md).

## Version pins

Researched 2026-07-12 ([14-traceability.md](docs/spec/14-traceability.md) §2). Change only via PR with re-research note.

| Component | Version | Note |
|---|---|---|
| Rust | 1.96.0 | `rust-toolchain.toml`, target `wasm32-unknown-unknown` |
| Leptos | 0.8.20 | stable line; 0.9 alpha not used |
| `worker` (workers-rs) | 0.8.5 | Cloudflare Worker SDK |
| Trunk | 0.21.14 | WASM bundler |
| wrangler | 4.110.0 | pinned in `package.json`; needs Node ≥ 22 |
| Terraform | 1.15.5 | local install |
| Cloudflare provider | 5.22.0 | Terraform, v5 line |
| FSRS candidate A | `fsrs` 6.6.1 | optimizer + scheduler, open-spaced-repetition; wasm32/size spike WBS 3.2 |
| FSRS candidate B | `rs-fsrs` 1.2.1 | scheduler only; last release 2024-10 — staleness risk |

FSRS GO/NO-GO decided by spike ([15-wbs.md](docs/spec/15-wbs.md) 3.2); fallback scheduler ships regardless.

## D1 free-tier limits (checked 2026-07-12)

500 MB per DB; 5 GB account; 10 DBs; 50 queries per Worker invocation; 100 bound params per query; 100 KB max SQL statement; 30 s max query; Time Travel 7 days. Fits single-user workload ([11-infrastructure.md](docs/spec/11-infrastructure.md) §7).

## Local environment note

`nvm` default node is 20.x; wrangler needs ≥ 22. Homebrew node 26.5.0 available — scripts prepend `/opt/homebrew/bin` to `PATH`. Or `nvm use system`.

## Commands

```sh
# Build + test (all crates, incl. migration/repository suites vs scratch SQLite)
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --check
cargo deny check

# Worker (wasm32) + local API with D1
cargo build -p worker-api --target wasm32-unknown-unknown
cd apps/worker && PATH="/opt/homebrew/bin:$PATH" npx wrangler d1 migrations apply DB --local
cd apps/worker && PATH="/opt/homebrew/bin:$PATH" npx wrangler dev --local   # http://localhost:8787

# Full local smoke (8-step M1 loop: profile → tutor-context → import → state)
bash scripts/smoke-local.sh

# Frontend release build + 3 MB budget check
bash scripts/wasm-size-report.sh

# Fuzz the untrusted lesson-result boundary (nightly)
cargo +nightly fuzz run lesson_result_parse -- -max_total_time=60
```
