# 16 — Rust Guidelines (Binding)

Scope: coding rules for all crates. Derived from project constraints and Rust review rules. Enforced by clippy configuration and code review.
Depends on: [02-workspace.md](02-workspace.md), [12-testing.md](12-testing.md).

## 1. Anti-overengineering rules

| Rule | Test |
|---|---|
| No abstraction before second consumer | Trait with one impl → concrete type. Exceptions: `ReviewScheduler`, `LanguagePack` (spec-mandated boundaries with 2 impls from M0: FSRS+fallback, synthetic+es-ES). |
| No new crate before first real code | Crate exists only when its first type lands ([02-workspace.md](02-workspace.md) §1). |
| Enum over trait object | Closed set of behaviors → `enum` + `match`, not `Box<dyn Trait>`. |
| No builder pattern for ≤ 4 fields | Struct literal or `new()`. |
| No generics for one concrete type | Monomorphic function first; generic only on second caller with different type. |
| No premature optimization | Measure first ([12-testing.md](12-testing.md) §8 targets). Exception: due-card index — spec-mandated. |
| No speculative config | Hardcode defaults from spec; config knob only when spec names it a setting. |
| Function ≤ 50 lines | Longer → split responsibilities. |
| No `pub` field with invariant | Private + constructor validation. Plain data DTO fields in `shared-schema` remain `pub`. |
| Delete, don't deprecate | Single-user app, no compatibility aliases ([00-overview.md](00-overview.md) §5). |

## 2. Error handling

Decision: expected failure → `Result`; normal absence → `Option`; bug/invariant violation → `panic!`/`assert!`.

| Layer | Strategy |
|---|---|
| Library crates (`learning-core`, `review-engine`, `language-pack-api`, …) | `thiserror` enums, precise variants, `#[from]` preserves source. No `anyhow`. |
| `apps/local` handlers | Map crate errors → stable operation error codes at the transport boundary ([08-local-rpc.md](08-local-rpc.md) §1). |
| `apps/web` | Typed error enum from `shared-schema`; rendered via `<ErrorBoundary>` ([09-frontend.md](09-frontend.md) §2). |

Rules:

- `.unwrap()`/`.expect()` forbidden in production code. `expect("reason")` allowed only for provably-infallible cases; reason states the invariant. Tests exempt.
- No `Box<dyn Error>` in public APIs — loses type info; use `thiserror` variants.
- Merge error variants that no caller handles differently.
- Propagate with `?`; add context at boundaries, not every hop.
- `ScheduleError` invariant violations (non-finite interval) → error, never panic — the native runtime must not crash during an operation.

## 3. Ownership and data

- No `.clone()` to escape borrow checker. Clone only for genuinely independent copies (e.g. before/after snapshots in `card_mutations`).
- Parameters: `&str` not `String`, `&[T]` not `&Vec<T>`. Store owned, borrow in.
- Iterators over index loops. No intermediate `collect()` when chain continues.
- Newtype IDs (`CardId(String)`, …) per [03-domain-model.md](03-domain-model.md) §1 — no bare `String` IDs crossing crate boundaries.
- Closed sets = enums, never stringly-typed. Strings only at serde/DB boundary, converted once.
- `std::sync::OnceLock` over `lazy_static!`. No `unsafe` anywhere in this workspace; a need for `unsafe` = design error.

## 4. Database (SQLite repositories)

- Repository layer (`apps/local/src/repositories/`) strictly separated from domain logic; SQL lives only there.
- Transaction boundary = domain invariant: review event + schedule state one batch; lesson completion one batch ([01-architecture.md](01-architecture.md) §4). Short transactions; no network calls inside.
- No N+1: queue fetch joins card + schedule state in one query.
- Additive migrations preferred; destructive → backup first + phased ([04-database.md](04-database.md) §4, [11-infrastructure.md](11-infrastructure.md) §5).
- Timezone/nullability handled once at repository boundary; domain types never hold raw TEXT timestamps.

## 5. Performance

Order of leverage: algorithm > data structure > allocations > micro. Work down, never up.

- Profile before optimizing; optimize only measured hot paths against [12-testing.md](12-testing.md) §8 targets. Complex code for < 20% gain rejected — readability wins.
- Pre-allocate known sizes: `Vec::with_capacity` for queue batches, export buffers.
- Batch SQLite writes in one transaction; do not add per-row query loops when a set operation works.
- No string concatenation in loops; build with capacity or `format!` once.
- For fixed sets used here (card grades, evidence kinds, pack lookups), use an enum-indexed array or `Vec` with linear scan unless a benchmark shows a `HashMap` win.
- Do not add rayon, thread pools, false-sharing padding, NUMA handling, jemalloc, or object pools without a measured requirement and benchmark.
- WASM size is the binding perf budget ([09-frontend.md](09-frontend.md) §9); dependency weight reviewed before any `cargo add` to `apps/web`.

## 6. Testing

- Unit tests: logic. Integration tests: contracts (repositories vs scratch SQLite, pack conformance). Property tests: invariants (scheduler intervals).
- Deterministic: no sleeps, no wall-clock — inject `Timestamp`; fixed seeds. Flaky test = bug, fix root cause.
- No over-mocking: repositories tested against real scratch SQLite, not mocked SQL.
- Both paths: every feature tests success + failure ([00-overview.md](00-overview.md) §6).
- Slow suites (Playwright, migration tests) out of default `cargo test` path; separate CI jobs.

## 7. Clippy enforcement

Workspace `Cargo.toml`:

```toml
[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "warn"      # allow-listed with invariant comment
panic = "warn"
cognitive_complexity = "warn"
```

CI: `cargo clippy --workspace --all-targets --all-features -- -D warnings` (gate 2, [11-infrastructure.md](11-infrastructure.md) §3). `#[allow]` requires a one-line justification comment.

## 8. Review checklist (PR)

- [ ] No unjustified `.clone()`, `.unwrap()`, index loop, `String` param
- [ ] No new trait/generic/crate/config without second consumer or spec mandate
- [ ] Errors: thiserror in crates, stable API codes at boundary, context preserved
- [ ] Transaction boundaries match domain invariants
- [ ] Success + failure tested; deterministic
- [ ] Function ≤ 50 lines; no `pub` invariant fields
- [ ] No unprofiled optimization; no server-class perf machinery (threads/pools/padding); SQLite writes transactional
- [ ] Core stays language-neutral ([12-testing.md](12-testing.md) §10)
