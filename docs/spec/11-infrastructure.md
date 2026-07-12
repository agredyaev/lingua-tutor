# 11 — Infrastructure, CI/CD, Operations

Scope: Terraform/Wrangler split, environments, pipelines, backup, observability.
Depends on: [01-architecture.md](01-architecture.md), [10-security.md](10-security.md).

## 1. Tool split

| Terraform manages | Wrangler manages |
|---|---|
| D1 databases + pack config metadata | Worker code deployment |
| Worker names + env config (where supported) | Static assets |
| Routes / custom domains | D1 migration execution |
| DNS (when custom domain) | Runtime secrets |
| Cloudflare Access app + policy (where supported) | |
| Non-secret bindings | |
| Protection settings | |

Terraform: Cloudflare Provider v5, pinned. Wrangler: pinned.

## 2. Environments

- `dev`: `lingua-tutor-dev` D1; free experimentation; never seeded from production data.
- `prod`: `lingua-tutor-prod` D1; `lifecycle { prevent_destroy = true }`; GitHub environment approval gate.

Any Terraform plan showing D1 replacement fails the deployment gate.

## 3. CI: pull request gate

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. unit + integration tests (`cargo test --workspace`; `cargo-nextest` where compatible)
4. scheduler golden tests
5. property tests
6. Leptos WASM build (Trunk, `wasm-release` profile) + binary size report
7. Worker build
8. D1 migration tests (scratch DB, apply all migrations)
9. Playwright smoke tests
10. `terraform fmt -check` + `terraform validate`
11. `cargo-deny` (deps + licenses) + dependency-direction check (no core crate → concrete pack)

## 4. CD: production deployment

1. approve production environment;
2. create or verify backup;
3. `terraform plan`;
4. reject any resource replacement;
5. apply approved infra changes;
6. `wrangler d1 migrations apply` (prod);
7. deploy Worker + assets;
8. health + smoke tests against prod;
9. record release metadata (version, schema version, migration ID).

## 5. Backup and recovery

Backed up: all D1 learning data — cards, review events, scheduler states, parameter sets, lessons, errors, audit records.

Requirements: regular automated export; encrypted or access-controlled storage; documented restore command; dry-run restore validation; RPO fit for daily personal use (≤ 24 h); restore test before production release and after every schema change. Backup required before any destructive migration.

## 6. Observability

Structured events (no full private content by default): lesson completed; cards created/mutated; review session completed; review event rejected; offline conflict; scheduler failure; import validation failure; migration completed; backup completed; MCP call outcome.

`GET /api/v1/health` reports: Worker status; D1 connectivity; schema version; scheduler algorithm/version; current parameter set; latest migration; deployment version.

## 7. Free-tier fit

Normal personal use stays within Cloudflare Workers Free + D1 free tier. Bounded queues, bounded exports, indexed due-card queries ([04-database.md](04-database.md)) keep request counts and row reads low. Any feature projected to exceed free tier requires explicit review.
