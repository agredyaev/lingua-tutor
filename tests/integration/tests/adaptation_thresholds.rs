//! Error-severity policy has one authoritative source in learning-core.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn error_severity_thresholds_are_impact_aware_and_single_source() {
    use learning_core::CommunicativeImpact::{High, Low, Medium};

    assert_eq!(learning_core::error_escalation_threshold(High), 2);
    assert_eq!(learning_core::error_escalation_threshold(Medium), 3);
    assert_eq!(learning_core::error_escalation_threshold(Low), 4);

    assert_eq!(learning_core::error_method_change_threshold(High), 4);
    assert_eq!(learning_core::error_method_change_threshold(Medium), 5);
    assert_eq!(learning_core::error_method_change_threshold(Low), 6);

    let adaptation_rules = include_str!("../../../crates/adaptation/src/rules.rs");
    assert!(!adaptation_rules.contains("ERROR_ESCALATION"));
    assert!(!adaptation_rules.contains("ERROR_METHOD_CHANGE"));
    assert!(!adaptation_rules.contains("error_escalation_action"));
}

fn stable_capability(rate: f64) -> adaptation::CapabilityEstimateFixed<'static> {
    adaptation::CapabilityEstimateFixed {
        attempts: 6,
        successes: 5,
        weighted_success_rate: Some(rate),
        unaided_success_rate: Some(rate),
        evidence_strength: 0.9,
        effective_evidence_weight: 5.0,
        unaided_evidence_weight: 5.0,
        unaided_evidence_episodes: 6,
        independent_evidence_episodes: 6,
        qualified_success_streak: 3,
        last_observed_qualified_success: Some(true),
        ..adaptation::CapabilityEstimateFixed::default()
    }
}

fn candidate_actions(
    item: &adaptation::ItemCapabilityStateFixed<'static>,
    objective: &str,
) -> Vec<&'static str> {
    let mut actions = Vec::new();
    adaptation::visit_task_candidates(
        adaptation::item_capability_view(item),
        objective,
        |candidate| {
            actions.push(candidate.task_type);
        },
    );
    actions
}

#[test]
fn adversarial_states_choose_measurement_only_when_it_can_change_the_next_action() {
    let strong_spontaneous = stable_capability(0.9);

    let listening_gap = adaptation::ItemCapabilityStateFixed {
        learning_item_id: "listen-gap",
        target_text: "target",
        status: "learning",
        listening: stable_capability(0.2),
        prompted_production: stable_capability(0.9),
        recognition: stable_capability(0.9),
        spontaneous_production: strong_spontaneous,
        transfer: adaptation::CapabilityEstimateFixed::default(),
    };
    let actions = candidate_actions(&listening_gap, "introduce_high_utility_material");
    assert!(actions.contains(&"listening_repair"));
    assert!(!actions.contains(&"listening_probe"));

    let mut sparse_prompted = adaptation::ItemCapabilityStateFixed {
        learning_item_id: "retrieval-gap",
        target_text: "target",
        status: "learning",
        listening: stable_capability(0.9),
        prompted_production: adaptation::CapabilityEstimateFixed {
            attempts: 1,
            successes: 1,
            weighted_success_rate: Some(1.0),
            evidence_strength: 0.25,
            effective_evidence_weight: 1.0,
            independent_evidence_episodes: 1,
            ..adaptation::CapabilityEstimateFixed::default()
        },
        recognition: stable_capability(0.9),
        spontaneous_production: stable_capability(0.2),
        transfer: adaptation::CapabilityEstimateFixed::default(),
    };
    sparse_prompted
        .spontaneous_production
        .qualified_success_streak = 0;
    let actions = candidate_actions(&sparse_prompted, "probe_prompt_independence");
    assert!(actions.contains(&"spontaneous_retrieval"));
    assert!(!actions.contains(&"fade_prompts"));

    let mut fragile = adaptation::ItemCapabilityStateFixed {
        learning_item_id: "fragile-boundary",
        target_text: "target",
        status: "learning",
        listening: stable_capability(0.9),
        prompted_production: stable_capability(0.9),
        recognition: stable_capability(0.9),
        spontaneous_production: stable_capability(0.67),
        transfer: adaptation::CapabilityEstimateFixed::default(),
    };
    fragile.spontaneous_production.attempts = 3;
    fragile.spontaneous_production.successes = 2;
    fragile.spontaneous_production.evidence_strength = 0.68;
    fragile.spontaneous_production.effective_evidence_weight = 2.72;
    fragile.spontaneous_production.unaided_evidence_weight = 2.72;
    fragile.spontaneous_production.unaided_evidence_episodes = 3;
    fragile.spontaneous_production.independent_evidence_episodes = 3;
    fragile.spontaneous_production.qualified_success_streak = 1;
    let actions = candidate_actions(&fragile, "probe_prompt_independence");
    assert!(actions.contains(&"diagnostic_spontaneous"));
    assert!(!actions.contains(&"spontaneous_retrieval"));

    let mut automaticity = adaptation::ItemCapabilityStateFixed {
        learning_item_id: "automaticity",
        target_text: "target",
        status: "learning",
        listening: stable_capability(0.9),
        prompted_production: stable_capability(0.9),
        recognition: stable_capability(0.9),
        spontaneous_production: stable_capability(0.9),
        transfer: adaptation::CapabilityEstimateFixed::default(),
    };
    automaticity.spontaneous_production.latency_weighted_millis = Some(9_000);
    automaticity
        .spontaneous_production
        .latency_evidence_episodes = 1;
    let actions = candidate_actions(&automaticity, "introduce_high_utility_material");
    assert!(!actions.contains(&"automaticity_drill"));

    automaticity
        .spontaneous_production
        .latency_evidence_episodes = 2;
    let actions = candidate_actions(&automaticity, "introduce_high_utility_material");
    assert!(actions.contains(&"automaticity_drill"));
}
