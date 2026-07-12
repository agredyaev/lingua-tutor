# 17 — Documentation & Engineering Standards

Scope: rustdoc rules; NASA-derived standards adapted to this project — NPR 7150.2D (current NASA Software Engineering Requirements) documentation discipline + JPL "Power of Ten" coding rules mapped to Rust. Adapted, not adopted wholesale: single-developer personal app ≈ NASA Class E software; full process bureaucracy would be overengineering ([16-rust-guidelines.md](16-rust-guidelines.md) §1).
Depends on: [16-rust-guidelines.md](16-rust-guidelines.md), [12-testing.md](12-testing.md).

## 1. Rustdoc standards

### Coverage

- Every crate: `//!` crate-level doc — one paragraph: responsibility, boundary, what it must not do (mirror [02-workspace.md](02-workspace.md) §2 table).
- Every public item (`pub` fn, struct, enum, trait, module): `///` doc. Enforced:

```toml
[workspace.lints.rust]
missing_docs = "warn"        # "deny" once M2 exit gate passes

[workspace.lints.rustdoc]
broken_intra_doc_links = "deny"
```

### Content rules

| Section | Required when |
|---|---|
| First line | Always. One sentence, verb-first, what it does — not how. |
| `# Errors` | Fn returns `Result` — list each variant condition. |
| `# Panics` | Fn can panic — forbidden in production paths ([16-rust-guidelines.md](16-rust-guidelines.md) §2), so presence of this section = review flag. |
| `# Examples` | Public API of `learning-core`, `review-engine`, `language-pack-api`, `tutor-contract`. Doc tests compile + run in CI. |
| `# Invariants` | Types with private fields guarding invariants (schedule state, state hash). |

Style: no restating signature; no "This function…" filler; intra-doc links (`[`CardId`]`) over prose references; caveman-terse — every doc sentence carries information.

### Doc gates (CI)

- `cargo doc --workspace --no-deps` warning-free (gate added to [11-infrastructure.md](11-infrastructure.md) §3 list as part of step 3 test run).
- Doc tests run with `cargo test --workspace`.

## 2. NPR 7150.2D-derived documentation discipline

Full NPR document set collapsed to what a Class-E solo project actually needs; each NASA artifact maps to an existing spec file — no new documents invented:

| NPR 7150.2D artifact | This project |
|---|---|
| Software Development/Management Plan | [15-wbs.md](15-wbs.md) + [11-infrastructure.md](11-infrastructure.md) |
| Software Requirements Specification | docs/spec.md v2.0 + files 00–13 |
| Software Design Description | 01–09 component files |
| Software Test Plan / Procedures | [12-testing.md](12-testing.md) |
| Version Description Document | release metadata recorded per deploy ([11-infrastructure.md](11-infrastructure.md) §4.9) |
| Requirements traceability | [14-traceability.md](14-traceability.md) — bidirectional: spec.md § → file; code PRs reference spec file § |
| Risk register | [13-roadmap.md](13-roadmap.md) §4 |
| Software Classification | Class E equivalent (research/personal, no safety impact) — recorded here |

Binding practices retained from NPR:

- **Traceability:** every PR description names the spec file + section it implements. Untraceable change = scope creep, rejected ([15-wbs.md](15-wbs.md) rules).
- **Versioned docs:** spec files change only via PR; `docs/spec.md` v2.0 stays immutable baseline.
- **ADRs:** every deviation from spec or major decision → ADR (started WBS 1.8). One page max.
- **Records over memory:** decisions, spikes (FSRS GO/NO-GO), release metadata — written, not remembered.

## 3. Power of Ten → Rust mapping (JPL, Holzmann)

Binding where mapped; Rust makes several rules free.

| # | JPL rule | Here |
|---|---|---|
| 1 | Simple control flow; no goto/setjmp | Free in Rust. Deep nesting → extract ([16-rust-guidelines.md](16-rust-guidelines.md) §1). |
| 2 | Fixed upper bound on every loop | Binding: all queue/import/replay loops bounded by spec limits (`max_review_batch`, size-bounded imports, bounded offline batches). No `loop` without break bound. |
| 3 | No recursion | Binding: none of the domain needs it; recursion = review flag. |
| 4 | No heap after init | Not adopted — Worker/WASM app; bounded allocations via rule 2 instead. |
| 5 | Function ≤ 60 lines | Already ≤ 50 ([16-rust-guidelines.md](16-rust-guidelines.md) §1). |
| 6 | Min two assertions per function | Adapted: invariants asserted at boundaries — schema validation, state-hash check, `debug_assert!` for internal invariants (scheduler transitions). Not a per-function quota. |
| 7 | Minimal scope for data | Rust default; no module-level mutable state, `OnceLock` for true singletons. |
| 8 | Check every return value | Free: `#[must_use]` + `Result` + clippy `unwrap_used = deny`. Ignoring a `Result` (`let _ =`) requires justification comment. |
| 9 | Limited preprocessor/pointer use | Adapted: macro discipline — no proc-macros of our own; declarative macros only for test tables. No `unsafe` (already binding). |
| 10 | All warnings on, zero tolerance | Already: `-D warnings`, clippy gate, `cargo-deny` ([11-infrastructure.md](11-infrastructure.md) §3). |

## 4. Review checklist additions (PR)

- [ ] Public items documented; doc tests pass; no broken intra-doc links
- [ ] `# Errors` present on every public `Result` fn
- [ ] PR references spec file + section (traceability)
- [ ] Loops bounded; no recursion; ignored `Result` justified
- [ ] Deviation from spec → ADR included in same PR
