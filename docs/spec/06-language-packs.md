# 06 — Language Packs

Scope: plugin contract (`language-pack-api`), registry, manifest, lifecycle, `synthetic-test-pack`, `language-pack-es-es`.
Depends on: [01-architecture.md](01-architecture.md) AD-9, [02-workspace.md](02-workspace.md).

## 1. Responsibility split

| Core owns | Pack owns |
|---|---|
| Profiles, lesson lifecycle, tutor-context assembly, tutor result validation, item/evidence/error history, card mutation workflow, review orchestration, scheduling persistence, analytics, offline reconciliation, auth, import/export/API/MCP boundaries | BCP 47 locale metadata; supported source languages; text normalization + canonical comparison; punctuation/capitalization/diacritic/contraction policies; accepted-answer matching; tokenization/morphology helpers; language error taxonomy + mapping to core categories; phrase/collocation/frame seeds; card-generation templates; tutor prompt fragments + correction conventions; pronunciation/transcription conventions; scenarios + cultural notes; CEFR descriptors; locale content labels (not app chrome) |

Core does not decide language-specific correctness, morphology, punctuation policy, or error classification.

A language pack must not:

- access SQLite;
- perform network calls;
- read secrets;
- bypass core validation;
- own scheduling state;
- write review events;
- define executable workflows.

## 2. Contract

Core owns all request/response types.
A pack cannot expose implementation types across the boundary.
Request/response types change through the contract-version process.
The responsibility split is fixed.

```rust
pub trait LanguagePack: Send + Sync {
    fn manifest(&self) -> &LanguagePackManifest;

    fn normalize_target_text(&self, input: &str, mode: NormalizationMode) -> NormalizedText;

    fn evaluate_answer(&self, request: &AnswerEvaluationRequest) -> AnswerEvaluation;

    fn classify_error(
        &self,
        request: &LanguageErrorClassificationRequest,
    ) -> LanguageErrorClassification;

    fn card_templates(&self) -> &[CardTemplate];

    fn tutor_instructions(&self, learner: &LearnerLanguageProfile) -> TutorInstructionBundle;

    fn curriculum_catalog(&self) -> &CurriculumCatalog;
}
```

Requirements: `normalize_target_text` deterministic; `evaluate_answer` bounded (no unbounded computation over input size); error codes stable across pack patch versions.

## 3. Manifest

```json
{
  "id": "es-ES",
  "package": "language-pack-es-es",
  "version": "1.0.0",
  "contract_version": "1",
  "target_language": "es",
  "target_locale": "es-ES",
  "display_name": "Spanish (Spain)",
  "supported_source_languages": ["ru", "en"],
  "default_source_language": "ru",
  "level_framework": "CEFR",
  "capabilities": [
    "production_cards",
    "listening_cards",
    "contrast_cards",
    "transfer_cards",
    "error_classification",
    "answer_matching"
  ]
}
```

## 4. Registry

```rust
pub fn built_in_language_packs() -> LanguagePackRegistry {
    LanguagePackRegistry::new([
        language_pack_es_es::register(),
    ])
}
```

- Compile-time registration; runtime selection by pack ID via `learner_language_profiles.language_pack_id`.
- Contract compatibility: core has integer `CONTRACT_VERSION`. Registry construction returns a version-mismatch error when an enabled pack declares another version.
- Only built-in compatible packs activatable in v1.
- Enabled pack metadata persisted to `language_packs` table for audit + historical attribution.

## 5. Lifecycle

- Pack IDs stable forever. Pack versions semver.
- Upgrades ship migration notes for changed content identifiers or taxonomies.
- Lessons, items, errors, cards keep the pack ID + version that produced them. Active-pack change never rewrites history; no silent re-normalization.

## 6. `synthetic-test-pack`

`synthetic-test-pack` implements the language-pack contract with artificial language rules.
Core CI uses it to test language-neutral behavior.
Production builds do not register it.

## 7. `language-pack-es-es`

Contains, as code + versioned declarative assets (`language-packs/es-es/assets/`):

| Asset | Content |
|---|---|
| `manifest.json` | §3 |
| normalization (code) | Spanish normalization: case, diacritics per policy, punctuation (¿¡), whitespace, contraction handling |
| answer matching (code) | Accepted-answer rules incl. regional alternatives |
| `error-taxonomy/` | Spanish error codes → core categories; correction guidance |
| `card-templates/` | Production, listening, contrast, transfer templates |
| `tutor-instructions/` | Prompt fragments, correction conventions; Russian learner-facing defaults |
| `curriculum/` | Scenarios for life in Spain; CEFR-mapped catalog |
| `phrase-seeds/` | Phrases/constructions for initial curriculum |

Target: contemporary Spanish of Spain.
Default source language: Russian.
Core crates do not contain Spanish-specific code.

## 8. Conformance suite

Every pack runs the shared `test-support` suite ([12-testing.md](12-testing.md) §5). It checks:

- manifest validity;
- contract-version compatibility;
- deterministic normalization;
- bounded answer evaluation;
- stable error codes;
- card-template validation;
- absence of secret and network access;
- asset schema validation;
- historical version attribution.

## 9. Non-goals v1

Out of scope:

- downloaded executable plugins;
- WASM plugins;
- plugin marketplace;
- user-authored executable plugins;
- independently deployed packs;
- per-pack databases;
- pack-owned scheduling algorithms.
