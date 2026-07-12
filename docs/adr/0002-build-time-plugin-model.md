# ADR-0002: Build-time registered language packs

Date: 2026-07-12. Status: accepted.

## Context

Target-language behavior must be pluggable (language-agnostic core). Options: runtime-loaded plugins (WASM modules, scripts) vs build-time registered Rust crates behind versioned trait.

## Decision

Packs = Rust crates implementing `LanguagePack` trait from `language-pack-api`, registered at build time in `language-pack-registry`, compatibility enforced by app-owned integer contract version. No runtime loading of plugin code.

## Consequences

- Single trusted binary; no plugin sandboxing, no dynamic loading machinery — fits one-user app and Worker runtime.
- Adding a language = new crate + registry entry + rebuild. Acceptable: packs are few and owner-authored.
- Core CI runs against `synthetic-test-pack`; conformance suite guards contract (WBS 2.5).
- Spec: docs/spec/01-architecture.md AD-5, 06-language-packs.md, spec.md §7.
