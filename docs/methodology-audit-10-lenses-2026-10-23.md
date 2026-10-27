# Methodology audit — 10 independent lenses

> **Superseded:** this is the pre-remediation 8.2/10 baseline. The final post-remediation audit is [methodology-audit-10-lenses-2026-10-23-final.md](methodology-audit-10-lenses-2026-10-23-final.md).

Date: 2026-10-23
Snapshot: current `refactor/core-data-path` working tree after methodology checkpoint and live MCP smoke test.

Method: ten isolated validation passes over the same evidence bundle. Each pass uses its own acceptance criteria and records findings before cross-lens synthesis. The synthesis section only appears after all ten reports.

External evidence considered: current Anki FSRS guidance; FSRS-6 reference documentation; Kim & Webb (2022) meta-analysis of spaced L2 practice; Rice & Tokowicz (2020) review of adult L2 vocabulary training; Pan & Rickard (2018) transfer of test-enhanced learning; Lyster & Saito (2010) oral corrective-feedback meta-analysis; Uchihara et al. (2025) HVPT meta-analysis.

## Validator 1 — FSRS / Anki correctness

Verdict: PASS WITH CAUTIONS.

Strengths:
- FSRS-6 is pinned and isolated; grade semantics match the reference model: Again=forgetting, Hard=successful but difficult recall.
- Persisted desired retention is actually passed into scheduling.
- Mature due reviews are ranked by ascending retrievability, appropriate for backlog risk.
- Maximum interval is no longer artificially capped at five years.
- The system does not falsely claim personalized FSRS parameters before an optimizer exists.

Findings:
- Profile validation accepts any finite desired retention in (0,1). Current default 0.90 is sound, but a future editable setting can accept pathological values near 1.0. Product policy should normally constrain or strongly warn above ~0.97.
- There is no explicit learning/relearning-step subsystem. This can be defensible because the lesson already performs immediate + delayed retrieval and fsrs-rs supports same-day state updates, but this should remain an empirically monitored v1 choice rather than an assumed optimum.
- Parameter optimization is correctly deferred, but an activation criterion should eventually be explicit (minimum effective review history, holdout improvement, rollback).

Severity: no blocker in the FSRS core itself.

## Validator 2 — Retrieval practice / spacing / memory science

Verdict: STRONG FOUNDATION, HEURISTIC CALIBRATION NEEDED.

Strengths:
- The tutor probes before showing the model.
- A failed attempt receives feedback/scaffolding and is followed by another retrieval attempt.
- Delayed retrieval is explicitly required.
- Review spacing and live-tutor spacing are separate, avoiding double ownership of the same memory process.
- The architecture encourages retrieval plus semantic elaboration rather than massed L1→L2 repetition.

Findings:
- The live tutor-focus schedule 0/1/3/7/14 days is a reasonable heuristic, not an evidence-established optimum. L2 spacing evidence supports distributed practice strongly but does not establish these exact gaps.
- Capability half-lives (recognition 60d, listening 35d, prompted production 21d, spontaneous 28d, transfer 42d) are policy constants without empirical calibration.
- `evidence_strength = decayed_weight / 4` is likewise heuristic. It works as an internal confidence proxy but should not be interpreted as calibrated probability.
- Limiting the projection to the newest 12 observations is operationally sensible but could make a stable skill too reactive after a cluster of unusual recent trials.

Severity: medium. Keep the design, calibrate constants from longitudinal data.

## Validator 3 — Second-language acquisition / communicative competence

Verdict: GOOD TASK-BASED DESIGN; TWO WEAK PROXIES HAVE TOO MUCH POWER.

Strengths:
- The target is successful real-life communicative action, not syllabus coverage.
- Phrases and response frames are preferred over isolated words.
- The lesson starts from learner production/listening evidence rather than CEFR label.
- L1 support is permitted when needed while maximizing comprehensible L2.
- One main bottleneck per lesson reduces diffuse correction and cognitive load.

Findings:
- `speaking_ratio < 0.45` produces priority 80. Speaking share is not equivalent to communicative competence; a learner can speak more while accomplishing less. It should be secondary to successful task completion/appropriacy.
- Tutor-estimated `confidence < 0.55` produces priority 82 and influences difficulty. This is a subjective assessor signal and currently outranks recurring errors and overdue retrieval. It should be demoted or used only as a tie-breaker unless calibrated.
- The model lacks a first-class objective communicative-outcome field such as task completed / repair needed / interlocutor goal achieved. This is closer to the declared product goal than speaking ratio.

Severity: high for planner validity, not for the overall tutoring philosophy.

## Validator 4 — Transfer and mastery validity

Verdict: CONCEPTUALLY EXCELLENT, BUT TWO MASTERY-GATE DEFECTS.

Strengths:
- Recognition, listening, prompted production, spontaneous production and transfer are separated.
- Assisted production cannot count as spontaneous/transfer evidence.
- Card success is not treated as proof of speaking mastery.
- Full mastery requires successful transfer across distinct semantic context families, which is directionally well aligned with transfer research.

Findings:
- `context_family` is free-form tutor text. Validation checks non-empty/length, not membership in a canonical taxonomy. Semantically identical labels such as `service.request_bill`, `restaurant.request_bill`, and `cafe.request_bill` can be counted as three distinct families and falsely satisfy mastery.
- Promotion to `mastered` checks accumulated successful family count >=3 but does not veto promotion when the current lesson contains a fresh failed transfer attempt. Historical breadth can therefore override contradictory current evidence.
- The exact threshold of three families is a product heuristic. Research supports varied retrieval/contextual variability, but not a universal magic number of three.

Required fix:
- Pack-owned canonical context-family IDs or a deterministic normalization/equivalence layer.
- A recent-failure/contradiction gate before accepting `mastered`.

Severity: HIGH; these are methodology correctness blockers.

## Validator 5 — Corrective feedback and recurring-error remediation

Verdict: STRONG FEEDBACK SEQUENCE, OVER-SIMPLIFIED ERROR POLICY.

Strengths:
- Self-repair is attempted before direct modeling.
- Prompting escalates gradually.
- After correction, the learner reproduces the whole natural utterance.
- The corrected form is tested again later.
- Repeated failure triggers a method change rather than endless repetition.
- This direction is consistent with evidence that corrective feedback is effective and that output-generating prompts can be especially useful.

Findings:
- Thresholds are universal: occurrence 3 => mini-target; occurrence 5 => method change; rechecks 1/3/7 days. The same policy applies to pronunciation, register, grammar, vocabulary, etc., despite different learning dynamics and communicative impact.
- The system does not distinguish a low-impact slip from an error that blocks communication when setting escalation thresholds.
- Concrete spec drift: `docs/spec/07-tutor-contract.md` uses `core_category: "verb_form"` in its example, but the actual closed enum is grammar/vocabulary/word_order/register/pronunciation/orthography/other.

Severity: medium-high. Feedback method is strong; escalation policy needs type/impact awareness.

## Validator 6 — Listening and perceptual learning

Verdict: SEPARATION IS CORRECT; VARIABILITY IS NOT OBSERVABLE ENOUGH.

Strengths:
- Listening is its own capability rather than inferred from recognition.
- Missing/weak listening evidence creates a dedicated listening probe.
- Tutor policy explicitly asks for a fresh variant and changing speaker characteristics/context.

Findings:
- The evidence model does not retain a stable talker/source-variant identifier. The system therefore cannot tell whether three listening successes came from the same synthetic voice and essentially the same acoustic realization.
- It cannot audit whether the requested talker/phonetic-context variability actually occurred.
- Current HVPT evidence supports varied talkers/phonetic contexts plus feedback for robust perceptual learning/generalization; the methodology asks for this, but the data model cannot validate it.
- Aggregate lesson `listening_score` is a coarse tutor-generated signal; item-level listening attempts are methodologically stronger and should dominate planning.

Required fix:
- Add non-audio metadata such as `source_variant_id`, optional `talker_key`, and/or controlled listening-context tags. No audio storage is necessary.

Severity: medium-high.

## Validator 7 — Speaking fluency and automaticity

Verdict: GOOD SIGNAL CHOICE, INVALID GLOBAL THRESHOLDING.

Strengths:
- Response latency is tracked separately from correctness.
- Prompt dependence is distinguished from unaided production.
- Slow-but-correct performance can create an automaticity task rather than more declarative explanation.

Findings:
- A global threshold of 5,000 ms is used for both planner and item-task logic. Five seconds has different meaning for a fixed phrase recall, a sentence transformation, and an open role-play response.
- Lesson-level `avg_answer_millis` mixes heterogeneous probe types and can create false “slow retrieval” conclusions.
- `long_pause_count` exists but is not part of the planner decision, even though it may carry useful complementary information.
- Latency should be compared within bounded task types and ideally relative to learner/item baseline.

Severity: medium-high.

## Validator 8 — Adaptive planner and policy coherence

Verdict: INTERPRETABLE AND TESTABLE, WITH ONE MAJOR POLICY CLIFF.

Strengths:
- Priority decisions are explicit, deterministic, bounded and explainable.
- Method-change/error-recheck rules correctly outrank optional new content.
- New-item count is a ceiling, not a quota.
- The tutor can deviate when live evidence contradicts the plan.

Findings:
- `overdue_reviews > 0` forces `max_new_items = 0`. A single newly overdue review therefore blocks all new lesson material. This is inconsistent with the card-budget backlog policy, which pauses new cards only at much higher pressure (>50 overdue) and otherwise uses graded throttling.
- Fixed priority scores create hard discontinuities around thresholds (e.g. 59% vs 60% unaided recall).
- `planner_history` and `intervention_effectiveness` are exposed to the tutor but do not alter deterministic planner scores. The closed-loop adaptation is therefore partly delegated to LLM compliance, not encoded in the policy.
- Subjective confidence and speaking-share signals currently outrank recurring-error and overdue-review priorities.

Required fix:
- Replace the binary overdue gate with a graded backlog function shared with review-budget pressure.
- Label planner thresholds as heuristic policy and add calibration/ablation tests before claiming optimization.

Severity: HIGH because the single-overdue cliff can systematically stall curriculum progress.

## Validator 9 — Measurement validity / psychometrics

Verdict: EXPLICIT EVIDENCE MODEL IS A MAJOR STRENGTH; ASSESSOR DEPENDENCE IS THE MAIN RISK.

Strengths:
- Unknown assistance is explicitly unknown and cannot prove independence.
- Evidence carries success, assistance, latency, phase, context and capability.
- Recency and evidence volume are represented separately from raw success.
- Immutable events make later recalibration possible.

Findings:
- The same tutor model often chooses the intervention, labels assistance/context, estimates confidence, and summarizes outcome. This creates correlated measurement error and a potential self-confirming adaptation loop.
- `confidence`, `speaking_seconds`, `listening_score`, and sometimes `response_millis` are not independently measured by the runtime.
- Free-form context families reduce construct validity of the mastery measure.
- Evidence-strength thresholds are deterministic but not statistically calibrated; values such as 0.45/0.65/0.70 should be treated as policy scores, not probabilities.

Recommendation:
- Prefer runtime-observable measurements where possible (turn timing, review grade, actual task outcome).
- Keep tutor-generated fields as weak evidence with lower decision weight.
- Build calibration reports comparing predicted capability state against later unaided/transfer outcomes.

Severity: high for future adaptive accuracy, moderate for current v1.

## Validator 10 — Methodology/spec/runtime consistency

Verdict: HIGH OVERALL CONSISTENCY, WITH SPEC DRIFT AND UNENFORCED GUIDANCE.

Strengths:
- Review spec, tutor instructions, adaptation code and live MCP behavior mostly agree.
- Live MCP smoke test behaved as intended: one successful spontaneous-production item produced a next-lesson plan with due retrieval, a missing-listening probe, and a delayed spontaneous diagnostic.
- Idempotent lesson completion behaved correctly.

Findings:
- Invalid `verb_form` core-category example in the tutor-contract spec is concrete drift.
- The recommended card-intent mix (production/listening/contrast/transfer percentages) is guidance only; no deterministic policy enforces or audits it. That is fine if documented as guidance, but it should not be presented as an invariant.
- The docs describe evidence-driven method changes, while deterministic planner adaptation from intervention effectiveness is not yet implemented; the tutor is responsible for interpreting that context.
- Several exact constants are presented as if canonical even though they are uncalibrated product heuristics.

Severity: medium.

# Blind synthesis

## Consensus strengths

At least 7/10 lenses independently support these design choices:
1. retrieval before reveal/model;
2. delayed retrieval and spacing;
3. explicit separation of recognition/listening/prompted/spontaneous/transfer;
4. real communicative contexts instead of syllabus completion;
5. prompt-first corrective feedback;
6. immutable evidence/history;
7. FSRS used only for card memory, not as a universal language-skill model.

## Consensus methodology blockers

### P0 — Canonicalize transfer context families
Validators 4, 6, 9 and 10 converge here. Free-form semantic family strings can inflate context diversity and therefore mastery.

### P0 — Recent contradictory evidence must block mastery promotion
Validators 4 and 9 independently reject a mastery gate that can accept three historical families while ignoring a fresh failed transfer.

### P0 — Remove the one-overdue-review curriculum cliff
Validators 3, 8 and 9 converge that `overdue_reviews > 0 => max_new_items=0` is disproportionate and inconsistent with the graded backlog policy.

## P1 findings

- Demote tutor-estimated confidence and raw speaking ratio as planner drivers; add communicative task outcome.
- Make latency task-relative rather than globally >5s.
- Add listening source/talker/context-variation metadata.
- Make error escalation sensitive to error type/communicative impact.
- Mark capability half-lives, evidence-strength cutoffs, transfer-family count=3 and 1/3/7 live spacing as explicit heuristics pending calibration.
- Fix `verb_form` spec drift.

## Non-blocking / accepted v1 choices

- FSRS defaults before enough history exists for parameter optimization.
- Desired retention default 0.90.
- No separate object-style learning-step subsystem, provided short-term behavior continues to be monitored.
- 100-year maximum interval.
- Deterministic fixed-capacity planner rather than an opaque learned policy.

# Overall judgment

Methodology quality: **8.2 / 10**.

The architecture is stronger than the methodology calibration. The core pedagogical model is coherent and evidence-aligned, but the three P0 items can produce materially wrong adaptive decisions even when all code is functioning exactly as designed. The system should not be labeled “methodology finalized” until those are fixed and covered by regression tests.
