# 06 — Language-neutral Tutor Boundary

Scope: separation between deterministic learning methodology and linguistic tutor behavior.
Depends on: [01-architecture.md](01-architecture.md), [07-tutor-contract.md](07-tutor-contract.md).

## 1. Responsibility split

| Engine owns | Tutor owns |
|---|---|
| profiles, lesson lifecycle, adaptation, evidence, mastery, error history, card mutation workflow, review orchestration, scheduling, persistence, validation | natural-language correctness, correction, linguistic interpretation, explanation, target-language generation, context-appropriate wording and pronunciation guidance |

The engine never decides target-language naturalness, morphology, punctuation tolerance, accent correctness, contractions or grammatical classification using language-specific heuristics.

The tutor never writes SQLite, mutates scheduling state directly, bypasses contract validation or performs hidden persistence.

## 2. Runtime language metadata

A language profile carries `target_locale` and `source_language`. These are scalar data, not handles to executable language behavior. Selecting or adding a target language does not rebuild the engine.

The runtime has no target-language crate, registry, virtual language dispatch, dynamic loader, WASM language module or rule DSL that substitutes for linguistic reasoning.

## 3. Processing boundary

Language-dependent reasoning is returned through the structured tutor contract. Core validates closed enums, bounds, ownership, references and operation invariants, then applies explicit processing passes to indexed state.

Core may canonicalize text only for technical storage/duplicate purposes. It must not silently lowercase, strip accents, remove meaningful punctuation, stem words, tokenize by language, or apply language-specific contractions.

Semantic context identifiers are tutor-provided opaque strings. Core may compare their identity for evidence rules but never interpret them linguistically.

## 4. Methodology instructions

Methodology instruction assets belong to the application because they describe the learning method rather than a target language. They may interpolate profile metadata such as target locale, source language and estimated level.

Architecture tests must reject target-language rules/examples embedded in methodology assets and reject reintroduction of executable language modules or dispatch layers.

## 5. Acceptance

A new target locale requires profile data only. No Cargo dependency, runtime dispatch entry, executable module, schema entity or rebuild is required.
