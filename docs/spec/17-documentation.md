# 17 — Documentation & Engineering Standards

Scope: active documentation and rustdoc rules.
Depends on: [16-rust-guidelines.md](16-rust-guidelines.md), [12-testing.md](12-testing.md).

## 1. Active documentation

Specs must describe the code that exists now. Historical implementation detail belongs in ADR context or migration tests, not in current component specs.

Executable sources of truth take precedence where duplication would drift:

- schema: `apps/local/migrations/` + migration tests;
- transport contracts: Rust types + validation tests;
- architecture constraints: integration architecture/dependency tests.

## 2. Rustdoc

Every crate and public item documents responsibility and boundary. Public `Result` APIs document failure conditions. Production panic paths are review failures. Public APIs in `learning-core`, `review-engine`, `tutor-contract` and `shared-schema` should have concise examples where they materially improve understanding.

CI gates:

```bash
cargo doc --workspace --no-deps
cargo test --workspace
```

## 3. Engineering documentation discipline

- Every architectural change updates its active spec and architecture gate in the same change.
- Major decisions use an ADR.
- Current specs do not retain obsolete architecture as a second source of truth.
- Migration compatibility is recorded by executable migration tests.
- PRs reference the relevant spec/ADR.
- Risks and release assumptions are written, not kept only in memory.

## 4. Review checklist

- public items documented where required;
- docs and code agree on workspace/schema/boundaries;
- no obsolete architecture remains in active specs;
- loops/batches are bounded;
- no hidden compatibility layer, object graph, service/factory/manager abstraction or language-specific core behavior is introduced.
