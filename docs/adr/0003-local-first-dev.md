# ADR-0003: Local-first development until production hardening

Date: 2026-07-12. Status: accepted. Deviation from 15-wbs.md 1.4/1.6 wording.

## Context

WBS 1.4 says "deployed to dev", 1.6 "dev database". Real Cloudflare resources require account auth mid-iteration and duplicate what Terraform provisions properly at WBS 10.1.

## Decision

Until WBS 10: `wrangler dev --local` (workerd + local D1/SQLite), placeholder `database_id` in `wrangler.jsonc`, smoke via `scripts/smoke-local.sh`. First real Cloudflare deploy happens through Terraform + CD pipeline at WBS 10.1–10.3.

## Consequences

- Zero credentials needed through M0–M4; identical Worker/D1 code paths (workerd is the production runtime).
- Risk: account-level surprises (limits, Access config) discovered at WBS 10 — mitigated by spec 11-infrastructure.md free-tier review and early Terraform plan.
- WBS rows 1.4/1.6 deliverables reworded to local equivalents; drift recorded in docs/wbs-log.md iteration 1.
