# ADR-0002: Build-time registered language packs

Date: 2026-07-12. Status: accepted.

## Context

Target-language behavior must be pluggable while the platform core stays language-neutral. Options were runtime-loaded plugins or build-time registered Rust crates behind a versioned contract.

## Decision

Language packs are Rust crates registered at build time in `language-pack-registry`. `language-pack-api` owns only shared data contracts; language behavior is exposed as explicit free processing passes in each concrete pack crate. The registry stores aligned pack data and manifests, resolves external IDs to dense `u32` indices, and performs no virtual dispatch. The app owns the integer contract version and rejects incompatible packs. Runtime loading of plugin code is not supported.

## Consequences

- One trusted binary; no plugin sandboxing or dynamic-loading machinery.
- Adding a language means adding a crate + registry entry + rebuild.
- Core CI uses `synthetic-test-pack` to prove language independence.
- This fits the trusted single-binary local runtime defined by ADR-0006.
