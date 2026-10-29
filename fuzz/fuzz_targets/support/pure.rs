use adaptation::{evidence_capability, new_card_budget, DailySignal};
use language_pack_api::{
    AnswerEvaluationRequest, LanguageErrorClassificationRequest, LanguagePackManifest,
    NormalizationMode,
};
use review_engine::{
    apply_review, initial_state, new_scheduler_scratch, Grade, ReviewEventInput, SchedulerSettings,
    Timestamp, FALLBACK_ID, FSRS_ID, MIN_INTERVAL_SECONDS,
};

fn text(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

pub fn exercise(data: &[u8]) {
    let a = text(data.get(..data.len().min(128)).unwrap_or(data));
    let split = data.len() / 2;
    let b = text(&data[split..]);

    for mode in [NormalizationMode::Canonical, NormalizationMode::Comparison] {
        let first = language_pack_es_es::normalize_target_text(&a, mode);
        let second = language_pack_es_es::normalize_target_text(&a, mode);
        assert_eq!(first, second);
    }

    let accepted = vec![b.clone(), a.trim().to_owned()];
    let request = AnswerEvaluationRequest {
        expected_answer: a.clone(),
        accepted_answers: accepted,
        learner_answer: b.clone(),
        evaluation_mode: "normalized".to_owned(),
    };
    let first = language_pack_es_es::evaluate_answer(&request);
    let second = language_pack_es_es::evaluate_answer(&request);
    assert_eq!(first, second);

    let classification_request = LanguageErrorClassificationRequest {
        original_production: a.clone(),
        corrected_production: b.clone(),
        context: Some("fuzz".to_owned()),
    };
    let (_, pack) = language_pack_es_es::register();
    assert_eq!(
        language_pack_es_es::classify_error(&pack, &classification_request),
        language_pack_es_es::classify_error(&pack, &classification_request)
    );

    let registry = language_pack_registry::build_registry([language_pack_es_es::register()])
        .expect("built-in pack must always register");
    assert!(language_pack_registry::get(&registry, "es-ES").is_some());
    assert!(language_pack_registry::manifest(&registry, "es-ES").is_some());
    let _ = language_pack_es_es::scenario_at(&pack, data.first().copied().unwrap_or(0) as u32);
    let _ = language_pack_es_es::seed_at(&pack, data.get(1).copied().unwrap_or(0) as u32);
    let _ = language_pack_es_es::card_template_at(&pack, data.get(2).copied().unwrap_or(0) as u32);

    if let Ok(manifest) = serde_json::from_slice::<LanguagePackManifest>(data) {
        let _ = language_pack_registry::build_registry([(manifest, ())]);
    }

    let (manifest, _) = language_pack_es_es::register();
    let mut manifest_doc = serde_json::to_value(&manifest).expect("manifest JSON");
    manifest_doc["contract_version"] = serde_json::json!(1);
    let _ = serde_json::from_value::<LanguagePackManifest>(manifest_doc);
    for value in ["de el parque", "a el parque", "áéíóúüÁÉÍÓÚÜ"] {
        let _ = language_pack_es_es::normalize_target_text(value, NormalizationMode::Canonical);
        let _ = language_pack_es_es::evaluate_answer(&AnswerEvaluationRequest {
            expected_answer: value.to_owned(),
            accepted_answers: vec![],
            learner_answer: "aeiouuAEIOUU".into(),
            evaluation_mode: "normalized".into(),
        });
    }
    let intent = match data.first().copied().unwrap_or(0) % 4 {
        0 => language_pack_api::CardIntent::Production,
        1 => language_pack_api::CardIntent::Listening,
        2 => language_pack_api::CardIntent::Contrast,
        _ => language_pack_api::CardIntent::Transfer,
    };
    let mut answers = vec![a.clone(), b.clone()];
    let hash_a = learning_core::card_content_hash(intent, &a, &b, &answers);
    answers.reverse();
    let hash_b = learning_core::card_content_hash(intent, &a, &b, &answers);
    assert_eq!(hash_a, hash_b);

    let count = u32::from(data.get(3).copied().unwrap_or(0));
    let resolved = data.get(4).copied().unwrap_or(0) & 1 == 1;
    let _ = learning_core::error_escalation_due(count, resolved);
    let _ = learning_core::error_method_change_due(count, resolved);

    let base = data.get(5).copied().unwrap_or(8).min(64);
    let yesterday = data.get(6).copied().unwrap_or(base).min(64);
    let overdue = u32::from(data.get(7).copied().unwrap_or(0));
    let daily = DailySignal {
        review_minutes: u16::from(data.get(8).copied().unwrap_or(0)),
        recall_rate: f64::from(data.get(9).copied().unwrap_or(128)) / 255.0,
    };
    let budget = new_card_budget(base, yesterday, overdue, 12, Some((daily, daily)));
    assert!(budget <= base);
    let _ = evidence_capability(&a);

    let _ = Grade::try_from(data.first().copied().unwrap_or(0));
    let _ = review_engine::new_scheduler_scratch(99);
    let _ = review_engine::initial_state(99, Timestamp(0));
    for scheduler in [FSRS_ID, FALLBACK_ID] {
        let scratch = new_scheduler_scratch(scheduler).expect("scheduler scratch");
        let settings = SchedulerSettings::default();
        let mut state = initial_state(scheduler, Timestamp(1_800_000_000)).expect("initial state");
        let _ =
            review_engine::predicted_retrievability(scheduler, &state, Timestamp(1_800_000_000));
        let _ = review_engine::predicted_retrievability(99, &state, Timestamp(0));
        let _ = review_engine::apply_review(
            99,
            &scratch,
            &state,
            &ReviewEventInput {
                grade: Grade::Good,
                answered_at: Timestamp(0),
            },
            &settings,
        );
        let mut events = Vec::new();
        let mut now = 1_800_000_000_i64;
        for byte in data.iter().copied().take(32) {
            now = now.saturating_add(600 + i64::from(byte) * 60);
            let grade = Grade::try_from(byte % 4 + 1).expect("bounded grade");
            let event = ReviewEventInput {
                grade,
                answered_at: Timestamp(now),
            };
            events.push(event);
            let next = apply_review(scheduler, &scratch, &state, &event, &settings)
                .expect("valid review sequence");
            let repeat = apply_review(scheduler, &scratch, &state, &event, &settings)
                .expect("deterministic repeat");
            assert_eq!(next, repeat);
            let interval = next.scheduled_interval_seconds.expect("interval");
            assert!(interval >= MIN_INTERVAL_SECONDS);
            assert_eq!(next.due_at.expect("due").0, now + interval);
            assert!(next.state_version > state.state_version);
            let recall =
                review_engine::predicted_retrievability(scheduler, &next, Timestamp(now + 3600))
                    .expect("recall");
            assert!((0.0..=1.0).contains(&recall));
            state = next;
        }
        assert_eq!(
            state,
            review_engine::replay(scheduler, Timestamp(1_800_000_000), &events, &settings)
                .expect("replay")
        );
    }
}
