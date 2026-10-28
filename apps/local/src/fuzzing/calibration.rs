use serde_json::{Value, json};
pub(super) fn exercise(seed: u8) {
    let capabilities = [
        "recognition",
        "listening",
        "prompted_production",
        "spontaneous_production",
        "transfer",
    ];
    let mut snapshots = Vec::new();
    for time in 0..4 {
        let mut items = Vec::new();
        let mut interventions = Vec::new();
        for item in 0..3 {
            let id = format!("item-{item}");
            let mut row = json!({"learning_item_id":id});
            for capability in capabilities {
                let rate = match (usize::from(seed) + time + item) % 5 {
                    0 => Value::Null,
                    1 => json!(0.2),
                    2 => json!(0.8),
                    3 => json!(0.81),
                    _ => json!(0.4),
                };
                row[capability] = json!({"weighted_success_rate":rate,"unaided_success_rate":rate,"last_observed_at_unix":time*100+1,"distinct_success_families":time+item});
                let intervention = json!({"action":if item%2==0 {"repeat"}else{"change"},"learning_item_id":id,"target_capability":capability});
                interventions.push(intervention.clone());
                interventions.push(intervention);
            }
            if time == 1 && seed & 8 != 0 {
                continue;
            }
            items.push(row);
        }
        interventions.push(json!({"action":"unknown","learning_item_id":"absent","target_capability":"correction"}));
        let outcome =
            json!({"calibration_valid":seed&16==0,"interventions":interventions}).to_string();
        snapshots.push(((time as i64) * 100000, json!(items).to_string(), outcome));
    }
    let summarize = || {
        adaptation::summarize_intervention_history(
            snapshots
                .iter()
                .map(|(at, items, outcome)| (*at, items.as_str(), outcome.as_str())),
            1000,
        )
    };
    let a = summarize();
    assert_eq!(a, summarize());
    for effect in a {
        assert_eq!(
            effect.samples,
            effect.improved + effect.stable + effect.declined
        );
    }
}
