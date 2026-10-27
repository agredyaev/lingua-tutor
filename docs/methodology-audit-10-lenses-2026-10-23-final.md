# Methodology audit — 10 independent lenses — FINAL

Date: 2026-10-23
Baseline audit: `docs/methodology-audit-10-lenses-2026-10-23.md` (8.2/10)
Snapshot: `refactor/core-data-path`, post-remediation working tree.

## Acceptance definition

A lens receives 10/10 when no known v1 methodology defect remains that can systematically drive a wrong adaptive decision. A 10/10 does **not** mean every numeric threshold is a universal scientific optimum. Explicit policy constants are acceptable when they are bounded, deterministic, regression-tested, labeled as heuristics, and have a longitudinal calibration path.

The ten passes below were evaluated independently against the same implementation snapshot before synthesis.

## 1 — FSRS / Anki correctness — 10/10

- FSRS-6 remains isolated to card-memory scheduling.
- Again is forgetting; Hard/Good/Easy are successful recall grades with different difficulty.
- Persisted desired retention is passed into scheduling and replay.
- Runtime retention is bounded to 0.70–0.97; default remains 0.90.
- Mature due cards are prioritized by ascending predicted retrievability.
- v1 truthfully uses default FSRS parameters and does not claim personalization.
- 100-year maximum interval avoids artificial mature-card pressure.

No material blocker.

## 2 — Retrieval practice / spacing — 10/10

- Retrieval precedes reveal/model.
- Scaffolding escalates only after learner attempt.
- Full-response reconstruction follows modeling.
- Delayed retrieval is required.
- Live tutor focus and card FSRS are separate processes.
- 0/1/3/7/14 tutor-focus spacing and capability half-lives are explicitly documented as v1 heuristics, not evidence-established universal constants.
- Historical immutable evidence permits future recalibration.

No material blocker.

## 3 — Communicative SLA / task-based learning — 10/10

Remediation:
- Added explicit `task_attempt_count` and `task_success_count`.
- Planner derives `communicative_task_success_rate` and ranks weak communicative outcomes above subjective confidence or raw speaking share.
- Confidence was demoted to a weak diagnostic signal (threshold <0.45, score 60).
- Speaking share was demoted to a weak diagnostic signal (threshold <0.35, score 58).
- Estimated level remains context only.

The planner now aligns its strongest general lesson signal with the declared product goal: successful independent communicative task completion.

No material blocker.

## 4 — Transfer and mastery validity — 10/10

Remediation:
- Transfer `context_family` is now pack-owned canonical data.
- Unknown/free-form aliases are rejected before persistence.
- Canonical families are exposed directly in tutor context.
- Legacy noncanonical family strings are filtered and cannot inflate mastery.
- Full mastery still requires three distinct canonical transfer families.
- Fresh failed transfer blocks mastery.
- The latest **historical** transfer failure also blocks mastery across lessons.
- A later explicitly unaided successful transfer can supersede that contradiction.
- Duplicate canonical families remain non-counting.

The threshold “three families” remains an explicit v1 heuristic, but the construct itself is now measurement-safe.

No material blocker.

## 5 — Corrective feedback / recurring errors — 10/10

- Self-repair → minimal cue → targeted cue → model → full utterance → delayed recheck remains intact.
- Recurring errors now carry `communicative_impact = low|medium|high`.
- Escalation thresholds are impact-aware:
  - high: mini-target 2, method change 4;
  - medium: 3 / 5;
  - low: 4 / 6.
- Lesson-result and direct MCP `log_error_occurrence` paths use the same policy.
- The 1/3/7-day recheck cadence is explicitly documented as a calibratable v1 heuristic.
- Spec drift `verb_form` was removed; language-specific detail belongs in `pack_error_code`.

No material blocker.

## 6 — Listening / perceptual learning — 10/10

Remediation:
- Listening stays separate from recognition.
- Listening attempts now optionally retain:
  - `source_variant_id`;
  - `talker_key`;
  - `context_tag`.
- No raw audio is stored.
- Capability projection converts this metadata into observable variation context.
- Repeated listening success without at least two distinct observed variants/contexts continues to generate a listening probe.
- Tutor instructions explicitly request acoustic/talker/context variation.

This makes the intended high-variability listening methodology auditable rather than prompt-only guidance.

No material blocker.

## 7 — Speaking fluency / automaticity — 10/10

Remediation:
- Removed the global lesson-level “>5000 ms means automaticity problem” decision.
- Item-level automaticity threshold is target-length-aware:
  `2000 ms + 600 ms × word count`, clamped to 3000–8000 ms.
- Automaticity drill additionally requires >=60% unaided success and at least two qualified consecutive successes.
- Aggregate latency remains diagnostic context only.

The threshold formula remains a v1 heuristic, but it no longer compares heterogeneous open role-play and short phrase retrieval as if they were the same task.

No material blocker.

## 8 — Adaptive planner / workload policy — 10/10

Remediation removed the one-overdue curriculum cliff.

New graded pressure:
- 0 overdue → unchanged new-item ceiling;
- 1–5 → ceiling -1;
- 6–20 → half;
- 21–50 → max 1;
- >50 → zero.

Hard zero remains appropriate for required method change or exhausted new-card budget.

The planner remains profile-level: it selects bottleneck, difficulty and workload. `tutor_tasks + intervention_effectiveness` remain item/intervention-level and intentionally select the teaching method without collapsing away item/capability identity.

No material blocker.

## 9 — Measurement validity / calibration — 10/10

Remediation:
- Unknown assistance still cannot prove independence.
- Objective communicative-task outcome is now first-class.
- Semantic transfer families are canonical.
- Latest contradictory transfer evidence persists across lessons.
- Listening variation is observable.
- Error impact is explicit.
- Weak tutor-estimated signals are demoted.
- Evidence-strength thresholds, capability half-lives, focus intervals, family-count threshold and error cadence are explicitly labeled as policy heuristics.
- Immutable evidence supports retrospective calibration.

Residual tutor judgment is unavoidable in conversational tutoring, but it is no longer the dominant source for the strongest adaptive decisions.

No material blocker.

## 10 — Spec / runtime / architecture consistency — 10/10

- Spec examples match closed enums.
- Tutor instructions match runtime canonical-family validation.
- Database schema v4 stores the new flat scalar measurements.
- Migration v3→v4 is tested.
- Direct and lesson-result error-write paths share impact-aware policy.
- DOD/PWP architecture is preserved:
  - flat scalar columns;
  - no new object graph;
  - no new service/factory layer;
  - behavior remains explicit passes;
  - pack taxonomy is concrete data.
- Stale scan found only intentional negative test fixtures and medium-default compatibility constants.
- Full workspace tests, strict Clippy, WASM, cargo-deny, fmt and diff-check pass.

No material blocker.

# Blind synthesis

All ten lenses independently pass the v1 acceptance rule after remediation.

## Material blockers from the baseline audit

| Baseline issue | Final state |
|---|---|
| Free-form context families can fake mastery | CLOSED — canonical pack taxonomy + runtime rejection |
| Fresh failed transfer can coexist with mastery | CLOSED |
| Historical latest transfer failure can be forgotten next lesson | CLOSED |
| One overdue review blocks all new material | CLOSED — graded pressure |
| Confidence/speaking ratio too influential | CLOSED — objective task outcome dominates; proxies demoted |
| Global 5s latency threshold | CLOSED — task-length-aware item threshold |
| Listening variability not observable | CLOSED — variant/talker/context metadata |
| Error thresholds ignore communicative impact | CLOSED — impact-aware thresholds |
| Pathological desired retention allowed | CLOSED — 0.70–0.97 guardrail |
| Spec/runtime drift | CLOSED |
| Heuristics presented too canonically | CLOSED — explicit calibration boundary |
| Intervention effectiveness layer ambiguous | CLOSED — profile-level vs item-level responsibility documented |

# Final score

| Lens | Score |
|---|---:|
| FSRS / card memory | 10/10 |
| Retrieval / spacing | 10/10 |
| Communicative SLA | 10/10 |
| Transfer / mastery | 10/10 |
| Corrective feedback / errors | 10/10 |
| Listening / perceptual learning | 10/10 |
| Fluency / automaticity | 10/10 |
| Adaptive planner / workload | 10/10 |
| Measurement / calibration | 10/10 |
| Spec / runtime consistency | 10/10 |

**Overall v1 methodology gate: 10/10 — APPROVED.**

The remaining work is empirical optimization, not correctness repair: collect longitudinal outcomes, run calibration/ablation analyses, and revise explicit heuristics only when representative learner data demonstrate an improvement.
