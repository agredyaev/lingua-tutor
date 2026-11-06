# ADR-0002: Language-neutral runtime

Date: 2026-07-12. Revised: 2026-11-05. Status: accepted.

## Context

An earlier design compiled target-language behavior into separate Rust crates and routed runtime work through a registry. That made linguistic behavior part of the executable and introduced composition/dispatch machinery even though linguistic interpretation is already performed by the tutor model.

## Decision

The runtime is language-neutral. It owns deterministic methodology only: learner state, lesson planning, adaptation, evidence, mastery, review scheduling, persistence, validation and bounded processing passes.

The tutor owns linguistic interpretation: correctness, correction, detailed error description, pronunciation guidance and lesson-content generation. Results cross the versioned tutor contract and are validated before deterministic state changes are applied.

A language profile stores only scalar source/explanation language and target locale. No executable behavior is selected from those values.

Technical text canonicalization is allowed only for storage/duplicate purposes and must not encode target-language morphology, punctuation tolerance, contractions, accents, tokenization or grammar rules.

No replacement plugin architecture is introduced.

## Consequences

- selecting a new target locale does not rebuild the binary;
- runtime/core code has no concrete target-language dependency;
- linguistic correctness is not inferred by ad-hoc language-specific heuristics in core;
- the repository has fewer crates, dispatch paths and ownership relationships;
- historical databases are migrated forward while preserving learning data.
