# ADR-0002: Build-time registered language packs

Date: 2026-07-12. Status: accepted.

## Context

Target-language behavior must remain outside language-neutral core crates.
The evaluated options were runtime-loaded plugins and build-time registered Rust crates behind a versioned contract.

## Decision

Language packs are Rust crates implementing `LanguagePack` from `language-pack-api`.
`language-pack-registry` registers them at build time.
The application owns the integer contract version and rejects incompatible packs.
The runtime does not load plugin code.

## Consequences

- One binary contains the registered packs. The runtime has no plugin sandbox or dynamic loader.
- Adding a language means adding a crate + registry entry + rebuild.
- Core CI uses `synthetic-test-pack` to prove language independence.
- Build-time registration matches the single-binary runtime defined by ADR-0006.
