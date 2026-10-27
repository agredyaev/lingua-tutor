# DOD/PWP release gate — 2026-10-23

## Decision

**DOD/PWP structural gate: APPROVED. Production release gate: PENDING environment-dependent evidence.**

This decision is based on the final `refactor/core-data-path` working tree. Functional correctness alone was not used as acceptance: the repository was audited for retained data layout, relationship IDs, processing dispatch, duplicate implementations, and architectural drift.

## Structural result

- One native business implementation remains: explicit `operations_v2` processing/write passes plus direct read/boundary projections.
- `apps/local/src/old` and `crates/application` are removed.
- `learning-core` no longer owns entity records/object graphs or string-ID wrappers; it contains only scalar contracts, timestamps, and pure rules/hashes with real consumers.
- Review scheduling uses explicit `u32` algorithm IDs and free passes; no scheduler behavior trait remains.
- Language-pack behavior uses concrete data plus free passes; no `LanguagePack` trait/dynamic dispatch remains.
- The pack registry is SoA/CSR storage with free lookup/build passes and dense `PackIndex = u32`.
- Adaptation retained data has no inherent processing methods; behavior is expressed as free passes.
- Lesson/card retained relationships are dense `u32`; `usize` is transient only at array/slice access.
- Architecture tests reject the removed object layers, dynamic dispatch, and retained relationship regressions.

## Verified gates

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --locked` | PASS |
| `cargo test --workspace --locked --quiet` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo check -p web --target wasm32-unknown-unknown --locked` | PASS |
| `cargo deny --locked check` | PASS |
| fuzz workspace `cargo check --locked` | PASS |
| fuzz workspace `cargo deny --locked ... check` | PASS |
| `lesson_result_parse` fuzz smoke | PASS, >=1,000 runs |
| Trunk `wasm-release` build | PASS |
| native debug build | PASS |
| native release build | PASS |
| `git diff --check` | PASS |

macOS `xcrun_db` cache warnings are sandbox warnings and did not fail compilation/tests.

The full fresh-SQLite RPC user cycle is also covered and passes in-process: create profile → `get_next_lesson` → `start_lesson` → `complete_lesson` → card admission → review session/queue → grade → complete review session → next lesson/dashboard. Verified SQLite backups are now restore-tested after simulated database loss. The Playwright core-loop mirrors the direct lesson lifecycle and additionally covers browser review, `/learn` next-lesson projection, card suspend→restore, offline grade→replay, and full-export snapshot validity.

## Release evidence not executable in this sandbox

Playwright cannot start because the sandboxed Node runtime fails before test execution with `EPERM` while resolving `/opt` (`node`/`npx` are under `/opt/homebrew`). A direct release-binary RPC benchmark was also attempted: project state/SQLite initialized, but the sandbox denied the localhost socket bind with `Operation not permitted`. The real OpenAI tunnel path additionally requires external tunnel credentials/service. Linux target evidence belongs to CI.

Representative in-process release-profile latency is now measured on 5,000 cards and 20,000 review events over 30 warmed samples. On the local macOS sandbox run, dashboard p95 was 11.43 ms (target <500 ms) and review-queue p95 was 8.73 ms (target <300 ms); the benchmark also asserts indexed access for the due-card query. CI runs the same named release benchmark. These numbers are environment-specific evidence, not a claim of cross-platform latency improvement. Browser Playwright execution, external tunnel→SQLite, and platform CI results remain environment-dependent release evidence. The built-in tunnel client has been re-audited against the current OpenAI base wire contract: unknown future command types are dropped without terminal responses, poll envelopes accept the current 10 MiB service bound, response deadlines use curl `time_starttransfer` rather than end-of-body time, `X-Tunnel-Client-Instance-Id` is stable for the client lifetime, and retryable poll/response failures use bounded exponential jitter plus valid `Retry-After` delta-seconds or HTTP-date values (capped at 60 seconds and still bounded by command deadlines). Optional `wrong-cluster-v1` routing correction is deliberately not advertised until its full state machine is implemented. Backup recovery itself is covered both by a native restore test and by the release-binary CI smoke path.

## CI hardening

The fuzz workflow now runs the fuzz workspace through `cargo-deny`, has a valid shell-step shape, and schedules both existing fuzz targets. The fuzz manifest declares its license and the deny policy explicitly permits the NCSA component required by `libfuzzer-sys`.

## Merge/release interpretation

The refactor is structurally compliant with the mandatory DOD/PWP rules and may be checkpointed/merged on that basis. A production release should still wait for the environment-dependent evidence above to pass in CI or a real runtime environment.
