//! Public read/write boundaries over a temporary, populated database.
use super::*;
use serde_json::{Value, json};

/// Fuzz imports, exports and both RPC adapters over disposable state.
pub fn exercise_boundaries(data: &[u8]) {
    let dir = std::env::temp_dir().join(format!("lingua-boundaries-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("fuzz directory");
    let path = dir.join("db.sqlite3");
    crate::storage::open(&path).expect("database");
    let env = Env::new(path.clone(), "fuzz-key".into());
    let registry = crate::built_in_language_packs().expect("registry");
    let profile = crate::operations::profile_write::create_profile(&registry, &env, "es-ES", "ru")
        .expect("profile");
    let id = profile["id"].as_str().expect("profile id");
    let version = language_pack_registry::manifest(&registry, "es-ES")
        .expect("manifest")
        .version
        .to_string();
    seed_fuzz_learning_item(&env, id);
    let result = rich_lesson_result(id, &version, "boundary-complete".into(), None, 0);
    let text = serde_json::to_string(&result).expect("document");
    let preview =
        crate::handlers::lessons::validate_import_text(&text, &env).expect("valid import");
    let token = preview["validation_token"].as_str().expect("token");
    let completion = crate::handlers::lessons::apply_import_text(&registry, &text, token, &env)
        .expect("apply import");
    let _ = no_storage_error(crate::handlers::lessons::apply_import_text(
        &registry, &text, "bad", &env,
    ));
    let _ = no_storage_error(crate::handlers::lessons::validate_import_text("{", &env));
    let mut invalid = result.clone();
    invalid.schema_version = "old".into();
    let _ = no_storage_error(crate::handlers::lessons::validate_import_text(
        &serde_json::to_string(&invalid).expect("invalid doc"),
        &env,
    ));
    let card = first_card_id(&env, id).expect("card");
    let lesson = env
        .connection()
        .expect("db")
        .query_row(
            "SELECT id FROM lessons WHERE learner_language_profile_id=?1 LIMIT 1",
            [id],
            |r| r.get::<_, String>(0),
        )
        .expect("lesson");
    let error = env
        .connection()
        .expect("db")
        .query_row(
            "SELECT id FROM error_targets WHERE learner_language_profile_id=?1 LIMIT 1",
            [id],
            |r| r.get::<_, String>(0),
        )
        .expect("error");
    let session =
        crate::operations::review_ops::create_session(&env, id, "online").expect("session");
    let at = crate::repositories::to_rfc3339(crate::api::now());
    let mut args = json!({"language_profile_id":id,"language_pack_id":"es-ES","source_language":"ru","pack_id":"es-ES","scenario":"fuzz",
    "lesson_id":lesson,"lesson_result":result,"text":text,"validation_token":token,
    "card_id":card,"card_ids":[card],"card_mutations":result.card_mutations,
    "core_category":"grammar","original_production":"wrong","corrected_production":"right","communicative_impact":"high",
    "tutor_summary":"corrected","next_goal":"goal","duration_seconds":20,"error_target_id":error,"recheck_id":"missing",
    });
    args.as_object_mut().expect("args").extend(json!({"passed":true,"context_tag":"fuzz","success":true,"assistance_level":0,"response_millis":500,"probe_phase":"diagnostic",
        "session_id":session["id"],"client_operation_id":"boundary-review","grade":3,"presented_at":at,"answered_at":at,
        "answer_revealed":false,"prior_state_hash":schedule_hash(&env,&card),"card_count":1,"review_seconds":20,"learner_answer":"card answer 0",
        "status":"review","edit_class":"cosmetic","prompt":"changed prompt","accepted_answers":["alt"],"days":14,"estimated_level":"A1"}).as_object().expect("args").clone());
    let seed = data.first().copied().unwrap_or(0);
    if seed & 1 != 0 {
        args["language_profile_id"] = json!("missing");
    }
    if seed & 2 != 0 {
        args["core_category"] = json!("invalid");
        args["communicative_impact"] = json!("invalid");
    }
    if seed & 4 != 0 {
        args["card_mutations"] = json!([]);
        args["card_ids"] = json!([card, "missing"]);
        args["lesson_result"] = json!({});
    }
    if seed & 8 != 0 {
        args["status"] = json!("invalid");
        args["grade"] = json!(0);
    }
    let tools = crate::mcp_tools::list();
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name");
        for arguments in [&args, &json!({})] {
            let request = json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":name,"arguments":arguments}});
            let response = crate::mcp::dispatch_jsonrpc(
                &env,
                &registry,
                &serde_json::to_vec(&request).expect("MCP request"),
            )
            .expect("MCP result");
            if let crate::mcp::DirectDispatch::Response(body) = response {
                let parsed: Value = serde_json::from_str(&body).expect("MCP JSON");
                assert_eq!(parsed["id"], 7);
            }
        }
    }
    for request in [
        json!({"id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"id":1,"method":"ping"}),
        json!({"id":1,"method":"tools/list"}),
        json!({"id":1,"method":"tools/call","params":{"name":"unknown"}}),
        json!({"id":1,"method":"tools/call"}),
        json!({"id":1,"method":"unknown"}),
        json!({"method":"ping"}),
    ] {
        let _ = crate::mcp::dispatch_jsonrpc(
            &env,
            &registry,
            &serde_json::to_vec(&request).expect("request"),
        );
    }
    let _ = crate::mcp::dispatch_jsonrpc(&env, &registry, data);
    let _ = crate::handlers::records::health(&env).expect("health");
    let _ = crate::handlers::records::learning_state_for(&env, id).expect("state");
    let _ = crate::handlers::records::get_error_target(&env, &error).expect("error details");
    let _ = crate::handlers::lessons::get_lesson(&env, &lesson).expect("lesson details");
    let _ = crate::handlers::cards::get(&env, &card).expect("card details");
    let full = crate::handlers::export::full_for(&env, id).expect("full export");
    let exported: Value = serde_json::from_slice(&full.body).expect("JSON export");
    assert_eq!(exported["language_profile_id"], id);
    let csv = crate::handlers::export::cards_csv_for(&env, id).expect("CSV export");
    assert!(!csv.body.is_empty());
    let _ = completion;
    crate::server::fuzz_boundary(&env, &registry, &dir, &args, data);
    negative_writes(&env, &registry, id, &error, &card);
    let db_error = crate::repositories::RepoError::from(rusqlite::Error::InvalidQuery);
    assert!(db_error.to_string().contains("database"));
    let app_error = crate::repositories::RepoError::from(crate::Error::RustError("fixture".into()));
    assert!(app_error.to_string().contains("fixture"));
    assert_database_integrity(&env);
    drop(env);
    std::fs::remove_dir_all(dir).expect("cleanup boundaries");
}

fn negative_writes(
    env: &Env,
    registry: &crate::BuiltInPackRegistry,
    profile: &str,
    error: &str,
    card: &str,
) {
    let make = |target: &str| crate::operations::log_error::LogErrorOccurrenceInput {
        language_profile_id: profile.into(),
        core_category: learning_core::CoreErrorCategory::Grammar,
        original_production: "wrong".into(),
        corrected_production: "right".into(),
        pack_error_code: None,
        explanation: None,
        error_target_id: Some(target.into()),
        communicative_impact: learning_core::CommunicativeImpact::High,
    };
    assert!(matches!(
        crate::operations::log_error::log_error_occurrence(registry, env, make("missing")),
        Err(Error::Operation {
            code: "not_found",
            ..
        })
    ));
    let db = env.connection().expect("db");
    db.execute("INSERT INTO learner_language_profiles(id,learner_id,language_pack_id,source_language,target_locale,status,is_default,created_at,updated_at) SELECT 'foreign-profile',learner_id,'es-ES','en','es-ES','archived',0,created_at,updated_at FROM learner_language_profiles WHERE id=?1",[profile]).expect("foreign archived profile fixture");
    let other = "foreign-profile";
    db.execute(
        "UPDATE error_targets SET learner_language_profile_id=?2 WHERE id=?1",
        rusqlite::params![error, other],
    )
    .expect("foreign target fixture");
    assert!(matches!(
        crate::operations::log_error::log_error_occurrence(registry, env, make(error)),
        Err(Error::Operation {
            code: "profile_mismatch",
            ..
        })
    ));
    db.execute(
        "UPDATE error_targets SET learner_language_profile_id=?2 WHERE id=?1",
        rusqlite::params![error, profile],
    )
    .expect("restore target");
    let mut result = rich_lesson_result(profile, "0.2.0", "cosmetic-fixture".into(), None, 0);
    let mut mutation = result.card_mutations.remove(0);
    mutation.operation = tutor_contract::MutationOperation::Update;
    mutation.client_operation_id = "standalone-cosmetic-unique".into();
    mutation.card_id = Some(card.into());
    let payload = mutation.card.as_mut().expect("payload");
    payload.prompt = "cosmetic standalone update".into();
    payload.answer = db
        .query_row("SELECT answer FROM cards WHERE id=?1", [card], |r| r.get(0))
        .expect("answer");
    crate::operations::upsert_cards::upsert_cards(registry, env, profile, &[mutation])
        .expect("cosmetic update");
    db.execute("INSERT INTO language_packs(id,package_name,version,contract_version,target_language,target_locale,display_name,installed_at,updated_at) VALUES('unregistered','unregistered','0.1.0',1,'zz','zz','Unavailable','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",[]).expect("unregistered pack fixture");
    db.execute(
        "UPDATE learner_language_profiles SET language_pack_id='unregistered' WHERE id=?1",
        [profile],
    )
    .expect("unregistered profile");
    assert!(matches!(
        crate::operations::log_error::log_error_occurrence(registry, env, make(error)),
        Err(Error::Operation {
            code: "pack_unavailable",
            ..
        })
    ));
    assert!(matches!(
        crate::operations::upsert_cards::upsert_cards(registry, env, profile, &[]),
        Err(Error::Operation {
            code: "pack_unavailable",
            ..
        })
    ));
    db.execute(
        "UPDATE learner_language_profiles SET language_pack_id='es-ES' WHERE id=?1",
        [profile],
    )
    .expect("restore profile");
}
