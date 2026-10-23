# 17 — Documentation & Engineering Standards

Scope: rustdoc, user documentation, traceability, and selected NPR 7150.2D / JPL Power of Ten rules. The project applies only the rules listed in this file.
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

Style: state behavior, constraint, invariant, or reason. Do not restate the signature. Avoid `This function...`. Use intra-doc links such as `[`CardId`]`.

### Doc gates (CI)

- `cargo doc --workspace --no-deps` warning-free (gate added to [11-infrastructure.md](11-infrastructure.md) §3 list as part of step 3 test run).
- Doc tests run with `cargo test --workspace`.


## 2. User documentation contract

User documentation starts from published release binaries and executable tasks.

- `README.md` lists supported platforms, release asset names, quick-start commands, and links to operational docs.
- `docs/installation.md` defines release-user installation, update, and uninstall steps.
- `docs/local-runtime.md` defines runtime, workspace, credential, and maintenance behavior.
- End-user installation must use published GitHub Release binaries. Rust/Cargo/Trunk/Node.js commands belong only in contributor, CI, or acceptance sections.
- The documented archive contract must match CI exactly: one `lingua-tutor` executable per ZIP plus `SHA256SUMS`.
- Every supported platform must have copy-pasteable checksum verification and installation commands.
- Unsupported targets must be stated explicitly; documentation must not imply binaries exist where CI does not publish them.
- Update instructions must preserve workspace data and tell the user to back up before replacing the executable.
- Security-sensitive bypasses such as macOS quarantine removal must be documented only after checksum verification, with the safety precondition stated next to the command.
- Runtime-created state must be distinguished from release-package contents.
- Troubleshooting must cover command discovery/PATH, architecture mismatch, occupied port, and workspace discovery.

CI enforces the key user-facing contract by rejecting `cargo install` in `README.md`, `docs/installation.md`, and `docs/local-runtime.md`.

## 3. NPR 7150.2D-derived documentation discipline

The table maps the retained NPR artifact categories to existing project files:

| NPR 7150.2D artifact | This project |
|---|---|
| Software Development/Management Plan | [15-wbs.md](15-wbs.md) + [11-infrastructure.md](11-infrastructure.md) |
| Software Requirements Specification | files 00–17 in `docs/spec/` |
| Software Design Description | 01–09 component files |
| Software Test Plan / Procedures | [12-testing.md](12-testing.md) |
| Version Description Document | release metadata recorded per deploy ([11-infrastructure.md](11-infrastructure.md) §4.9) |
| Requirements traceability | [14-traceability.md](14-traceability.md) — bidirectional: spec.md § → file; code PRs reference spec file § |
| Risk register | [13-roadmap.md](13-roadmap.md) §4 |
| Software Classification | Class E equivalent (research/personal, no safety impact) — recorded here |

Binding practices retained from NPR:

- **Traceability:** every PR description names the spec file + section it implements. Untraceable change = scope creep, rejected ([15-wbs.md](15-wbs.md) rules).
- **Versioned docs:** active spec files change with the code they describe; no obsolete baseline is kept as a second source of truth.
- **ADRs:** every deviation from spec or major decision → ADR (started WBS 1.8). One page max.
- **Records over memory:** decisions, spikes (FSRS GO/NO-GO), release metadata — written, not remembered.

## 4. Power of Ten → Rust mapping (JPL, Holzmann)

Binding where mapped; Rust makes several rules free.

| # | JPL rule | Here |
|---|---|---|
| 1 | Simple control flow; no goto/setjmp | Rust has no goto/setjmp control flow here. Split code when nesting hides branch conditions ([16-rust-guidelines.md](16-rust-guidelines.md) §1). |
| 2 | Fixed upper bound on every loop | Binding: all queue/import/replay loops bounded by spec limits (`max_review_batch`, size-bounded imports, bounded offline batches). No `loop` without break bound. |
| 3 | No recursion | Binding: none of the domain needs it; recursion = review flag. |
| 4 | No heap after init | Not adopted. The application permits bounded allocations. |
| 5 | Function ≤ 60 lines | Already ≤ 50 ([16-rust-guidelines.md](16-rust-guidelines.md) §1). |
| 6 | Min two assertions per function | Adapted: invariants asserted at boundaries — schema validation, state-hash check, `debug_assert!` for internal invariants (scheduler transitions). Not a per-function quota. |
| 7 | Minimal scope for data | No module-level mutable state. Use `OnceLock` for process-wide immutable initialization. |
| 8 | Check every return value | Free: `#[must_use]` + `Result` + clippy `unwrap_used = deny`. Ignoring a `Result` (`let _ =`) requires justification comment. |
| 9 | Limited preprocessor/pointer use | Adapted: macro discipline — no proc-macros of our own; declarative macros only for test tables and mechanical ID newtypes (`learning-core/src/ids.rs`). No `unsafe` (already binding). |
| 10 | All warnings on, zero tolerance | CI runs clippy with `-D warnings` and runs `cargo-deny` ([11-infrastructure.md](11-infrastructure.md) §3). |

## 5. Review checklist additions (PR)

- [ ] Public items documented; doc tests pass; no broken intra-doc links
- [ ] `# Errors` present on every public `Result` fn
- [ ] PR references spec file + section (traceability)
- [ ] Loops bounded; no recursion; ignored `Result` justified
- [ ] Deviation from spec → ADR included in same PR
- [ ] User-facing installation matches the current release asset contract
- [ ] No Cargo-based installation is presented as the release-user path
