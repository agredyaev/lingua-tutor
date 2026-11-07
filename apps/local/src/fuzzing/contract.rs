//! Structured invalid documents complement arbitrary-byte parser fuzzing.
use super::*;
use serde_json::{Value, json};

/// Fuzz parsed invalid lesson-result documents and deterministic validation.
pub fn exercise_contract(data: &[u8]) {
    let seed = data.first().copied().unwrap_or(0);
    let mut doc = serde_json::to_value(rich_lesson_result("p", "op".into(), None, 0))
        .expect("serialize fixture");
    let long = "ñ".repeat(tutor_contract::bounds::MAX_LONG_TEXT + 1);
    match seed % 20 {
        0 => {
            doc["schema_version"] = json!("unknown");
            doc["client_operation_id"] = json!("");
            doc["language_profile_id"] = json!(long);
            doc["lesson"]["scenario"] = json!("");
        }
        1 => {
            for field in [
                "new_learning_items",
                "evidence",
                "interventions",
                "error_occurrences",
                "listening_attempts",
                "mastery_changes",
                "card_mutations",
            ] {
                let entry = doc[field][0].clone();
                doc[field] = json!(vec![entry; 257]);
            }
        }
        2 => {
            doc["new_learning_items"] = json!([
            {"client_item_key":"", "item_type":"phrase", "target_text":""},
            {"client_item_key":"dup", "item_type":"phrase", "target_text":long},
            {"client_item_key":"dup", "item_type":"phrase", "target_text":"ok"}]);
            doc["lesson"]["tutor_summary"] = json!(long);
            doc["lesson"]["next_goal"] = json!(long);
            doc["error_occurrences"][0]["original_production"] = json!(long);
        }
        3..=7 => {
            let n = seed % 5;
            for evidence in doc["evidence"].as_array_mut().expect("evidence") {
                evidence["learning_item_id"] = if n == 0 { Value::Null } else { json!(long) };
                evidence["client_item_key"] = if n == 1 {
                    json!("missing")
                } else if n == 2 {
                    json!(long)
                } else {
                    Value::Null
                };
                evidence["kind"] = json!(
                    [
                        "recognition",
                        "prompted_production",
                        "spontaneous_production",
                        "transfer",
                        "correction"
                    ][usize::from(n)]
                );
                evidence["assistance_level"] =
                    [Value::Null, json!(0), json!(4), json!(1), json!(0)][usize::from(n)].clone();
                evidence["response_millis"] = if n & 1 == 0 {
                    Value::Null
                } else {
                    json!(600001)
                };
                evidence["probe_phase"] = Value::Null;
                evidence["context"] = if n & 1 == 0 { Value::Null } else { json!(long) };
                evidence["context_family"] = if n & 1 == 0 { json!("") } else { json!(long) };
                evidence["pronunciation_intelligible"] =
                    if n & 1 == 0 { Value::Null } else { json!(true) };
                evidence["pronunciation_repeat_required"] =
                    if n & 1 == 0 { Value::Null } else { json!(true) };
            }
        }
        8 => {
            let a = &mut doc["listening_attempts"][0];
            for field in ["source_variant_id", "talker_key", "context_tag"] {
                a[field] = json!("");
            }
            a["learning_item_id"] = json!("known");
            a["client_item_key"] = json!("missing");
            a["assistance_level"] = json!(4);
            a["response_millis"] = json!(600001);
            a["probe_phase"] = Value::Null;
        }
        9 => {
            let a = &mut doc["listening_attempts"][0];
            a["assistance_level"] = Value::Null;
            a["response_millis"] = Value::Null;
            a["context_tag"] = Value::Null;
        }
        10..=12 => {
            let mut all = Vec::new();
            for i in 0..8 {
                let mut v = doc["interventions"][0].clone();
                v["action"] = json!("");
                v["learning_item_id"] = if i & 1 == 0 { json!(long) } else { Value::Null };
                v["client_item_key"] = if i & 1 == 0 {
                    json!(long)
                } else {
                    json!("missing")
                };
                v["target_capability"] = json!(if i & 1 == 0 {
                    "correction"
                } else {
                    "recognition"
                });
                v["evidence_index"] = if i % 3 == 0 {
                    json!(255)
                } else if i % 3 == 1 {
                    json!(1)
                } else {
                    Value::Null
                };
                v["listening_attempt_index"] = if i % 3 == 0 {
                    json!(1)
                } else if i % 3 == 1 {
                    Value::Null
                } else {
                    json!(i)
                };
                all.push(v);
            }
            doc["interventions"] = json!(all);
        }
        13 => {
            let mut all = Vec::new();
            for op in [
                "create",
                "update",
                "add_accepted_answer",
                "remove_accepted_answer",
            ] {
                all.push(json!({"operation":op,"client_operation_id":""}));
            }
            let mut m = doc["card_mutations"][0].clone();
            for f in ["prompt", "answer", "creation_reason"] {
                m["card"][f] = json!("");
            }
            m["card"]["accepted_answers"] = json!(vec!["a"; 65]);
            m["card"]["scenario_tags"] = json!(vec!["tag"; 65]);
            m["card"]["priority_score"] = json!(-1);
            all.push(m);
            doc["card_mutations"] = json!(all);
        }
        14 | 15 => {
            doc["metrics"]["task_attempt_count"] = json!(if seed & 1 == 0 { 0 } else { 2 });
            doc["metrics"]["task_success_count"] = json!(3);
            if seed & 1 != 0 {
                doc["metrics"]["task_success_rubric"] = Value::Null;
            }
            doc["metrics"]["listening_score"] = json!(2);
            doc["metrics"]["confidence"] = json!(-1);
        }
        16 => {
            doc["mastery_changes"] = json!([
                {"learning_item_id":"known","new_status":"candidate"},
                {"learning_item_id":"known","new_status":"learning"}
            ]);
        }
        17 => {
            let long_id = "x".repeat(tutor_contract::bounds::MAX_ID + 1);
            let long_context = "x".repeat(tutor_contract::bounds::MAX_SHORT_TEXT + 1);
            doc["error_rechecks"] = json!([
                {
                    "error_target_id":"",
                    "recheck_id":"",
                    "passed":true,
                    "assistance_level":1,
                    "context":"",
                    "repair_applied":true,
                    "repair_action":"repair"
                },
                {
                    "error_target_id":long_id,
                    "recheck_id":"duplicate",
                    "passed":false,
                    "assistance_level":0,
                    "context":long_context,
                    "repair_applied":false,
                    "repair_action":"unexpected"
                },
                {
                    "error_target_id":"target",
                    "recheck_id":"duplicate",
                    "passed":false,
                    "assistance_level":0,
                    "context":"context",
                    "repair_applied":true,
                    "repair_action":""
                },
                {
                    "error_target_id":"target",
                    "recheck_id":"long-action",
                    "passed":false,
                    "assistance_level":0,
                    "context":"context",
                    "repair_applied":true,
                    "repair_action":"x".repeat(tutor_contract::bounds::MAX_ID + 1)
                },
                {
                    "error_target_id":"target",
                    "recheck_id":"invalid-action",
                    "passed":false,
                    "assistance_level":0,
                    "context":"context",
                    "repair_applied":true,
                    "repair_action":"Not_Canonical"
                },
                {
                    "error_target_id":"target",
                    "recheck_id":"missing-action",
                    "passed":false,
                    "assistance_level":0,
                    "context":"context",
                    "repair_applied":true
                }
            ]);
        }
        18 => {
            let long_id = "x".repeat(tutor_contract::bounds::MAX_ID + 1);
            doc["recommendation_resolutions"] = json!([
                {"recommendation_id":"","status":"accepted","repair_action":""},
                {"recommendation_id":long_id,"status":"dismissed","repair_action":"repair"},
                {"recommendation_id":"duplicate","status":"accepted","repair_action":"Not_Canonical"},
                {"recommendation_id":"duplicate","status":"accepted","repair_action":"x".repeat(tutor_contract::bounds::MAX_ID + 1)}
            ]);
        }
        _ => {
            let evidence = doc["evidence"][0].clone();
            let listening = doc["listening_attempts"][0].clone();
            doc["evidence"] = json!([evidence]);
            doc["listening_attempts"] = json!([listening]);
            let template = doc["interventions"][0].clone();
            let mut both = template.clone();
            both["evidence_index"] = json!(0);
            both["listening_attempt_index"] = json!(0);
            let mut missing_evidence = template.clone();
            missing_evidence["evidence_index"] = json!(255);
            missing_evidence["listening_attempt_index"] = Value::Null;
            let mut mismatched_evidence = template.clone();
            mismatched_evidence["evidence_index"] = json!(0);
            mismatched_evidence["listening_attempt_index"] = Value::Null;
            mismatched_evidence["target_capability"] = json!("transfer");
            mismatched_evidence["learning_item_id"] = json!("different");
            mismatched_evidence["client_item_key"] = Value::Null;
            let mut missing_listening = template.clone();
            missing_listening["evidence_index"] = Value::Null;
            missing_listening["listening_attempt_index"] = json!(255);
            let mut mismatched_listening = template;
            mismatched_listening["evidence_index"] = Value::Null;
            mismatched_listening["listening_attempt_index"] = json!(0);
            mismatched_listening["target_capability"] = json!("recognition");
            mismatched_listening["learning_item_id"] = json!("different");
            mismatched_listening["client_item_key"] = Value::Null;
            doc["interventions"] = json!([
                both,
                missing_evidence,
                mismatched_evidence,
                missing_listening,
                mismatched_listening
            ]);
        }
    }
    let text = doc.to_string();
    if let Ok(result) = tutor_contract::parse_lesson_result(&text) {
        let a = tutor_contract::validate(&result);
        let b = tutor_contract::validate(&result);
        assert_eq!(a, b);
        assert!(!a.is_empty(), "invalid structured fixture must be rejected");
        let _ = tutor_contract::measurement_coverage_counts(&result);
        assert_eq!(
            tutor_contract::communicative_task_rubric_id(
                tutor_contract::CommunicativeTaskRubric::GoalAchievementV1
            ),
            tutor_contract::COMMUNICATIVE_TASK_SUCCESS_RUBRIC_V1
        );
        assert_eq!(
            tutor_contract::communicative_task_rubric_id(
                tutor_contract::CommunicativeTaskRubric::IndependentGoalAchievementV2
            ),
            tutor_contract::COMMUNICATIVE_TASK_SUCCESS_RUBRIC_V2
        );
        assert_eq!(
            tutor_contract::recommendation_resolution_status_id(
                tutor_contract::RecommendationResolutionStatus::Accepted
            ),
            "accepted"
        );
        assert_eq!(
            tutor_contract::recommendation_resolution_status_id(
                tutor_contract::RecommendationResolutionStatus::Dismissed
            ),
            "dismissed"
        );
        for p in [
            tutor_contract::ProbePhase::Diagnostic,
            tutor_contract::ProbePhase::Practice,
            tutor_contract::ProbePhase::DelayedRetrieval,
            tutor_contract::ProbePhase::Transfer,
        ] {
            assert!(!tutor_contract::probe_phase_id(p).is_empty());
        }
    } else {
        panic!("structured contract fixture must remain parseable");
    }
    let _ = tutor_contract::parse_lesson_result(&String::from_utf8_lossy(data));
}
