//! Recommendation endpoints + persistence helper (WBS 8.2–8.3).
//! Paths defined here (08-http-api.md lists the dashboard surfacing;
//! resource endpoints follow its §1 conventions): GET /recommendations,
//! POST /recommendations/{id}/accept|dismiss, GET /cards/redesign-candidates.

use crate::api::{db, error_response, new_id, now, query_param};
use crate::repositories::{recommendations as repo, to_rfc3339};
use adaptation::Recommendation;
use worker::{D1Database, Env, Request, Response, Result};

/// Persists recommendations, skipping ones already open for the same
/// subject + action (no spam on every session).
pub async fn insert_all(
    database: &D1Database,
    profile_id: &str,
    recommendations: &[Recommendation],
) -> Result<u32> {
    #[derive(serde::Deserialize)]
    struct IdRow {
        #[allow(dead_code)]
        id: String,
    }
    let mut inserted = 0;
    for rec in recommendations {
        let duplicate = database
            .prepare(repo::SELECT_OPEN_DUPLICATE)
            .bind(&[
                profile_id.into(),
                rec.subject_type.into(),
                rec.subject_id.clone().into(),
                rec.action.into(),
            ])?
            .first::<IdRow>(None)
            .await?;
        if duplicate.is_some() {
            continue;
        }
        database
            .prepare(repo::INSERT)
            .bind(&[
                new_id("rec").into(),
                profile_id.into(),
                rec.subject_type.into(),
                rec.subject_id.clone().into(),
                rec.action.into(),
                serde_json::json!({ "detail": rec.detail })
                    .to_string()
                    .into(),
                to_rfc3339(now()).into(),
            ])?
            .run()
            .await?;
        inserted += 1;
    }
    Ok(inserted)
}

/// `GET /api/v1/recommendations?language_profile_id={id}`.
pub async fn list(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let rows: Vec<serde_json::Value> = db(env)?
        .prepare(repo::SELECT_OPEN)
        .bind(&[profile_id.into(), 50.into()])?
        .all()
        .await?
        .results()?;
    Response::from_json(&rows)
}

/// `POST /api/v1/recommendations/{id}/accept` | `/dismiss`.
pub async fn resolve(env: &Env, recommendation_id: &str, status: &str) -> Result<Response> {
    db(env)?
        .prepare(repo::UPDATE_STATUS)
        .bind(&[
            recommendation_id.into(),
            status.into(),
            to_rfc3339(now()).into(),
        ])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({ "id": recommendation_id, "status": status }))
}

/// `GET /api/v1/cards/redesign-candidates?language_profile_id={id}`.
pub async fn redesign_candidates(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let rows: Vec<serde_json::Value> = db(env)?
        .prepare(repo::SELECT_REDESIGN_CANDIDATES)
        .bind(&[profile_id.into()])?
        .all()
        .await?
        .results()?;
    Response::from_json(&rows)
}
