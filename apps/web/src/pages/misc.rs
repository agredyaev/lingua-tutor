//! Learn, settings, system pages.

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::{ApiError, HealthResponse};

/// `/learn`: the no-MCP lesson loop, step by step (13-roadmap.md §2).
#[component]
pub fn LearnPage() -> impl IntoView {
    let active = use_active_profile();
    let next = LocalResource::new(move || {
        let id = active.id();
        async move {
            let Some(id) = id else {
                return Ok::<Option<serde_json::Value>, ApiError>(None);
            };
            api_client::call::<_, serde_json::Value>(
                "get_next_lesson",
                &serde_json::json!({ "language_profile_id": id }),
            )
            .await
            .map(Some)
        }
    });

    view! {
        <Page>
            <header class="page-header">
                <div>
                    <p class="eyebrow">"Tutor handoff"</p>
                    <h1>"Next lesson"</h1>
                    <p class="page-description">
                        "Use the persisted learner state as the handoff between the tutor and the local engine."
                    </p>
                </div>
            </header>

            <Suspense fallback=|| view! { <p class="loading">"Preparing next lesson…"</p> }>
                {move || {
                    next.get()
                        .map(|result| match result {
                            Ok(Some(n)) => {
                                view! {
                                    <section class="panel lesson-preview">
                                        <span class="section-kicker">"Next session"</span>
                                        <p class="lesson-preview-number">
                                            {format!("Lesson #{}", n["lesson_number"])}
                                        </p>
                                        {n["goal"]
                                            .as_str()
                                            .map(|g| {
                                                view! {
                                                    <div class="detail-block">
                                                        <span class="section-kicker">"Goal"</span>
                                                        <p>{g.to_owned()}</p>
                                                    </div>
                                                }
                                            })}
                                    </section>
                                }
                                    .into_any()
                            }
                            Ok(None) => {
                                view! {
                                    <div class="empty-state">
                                        <p class="empty-state-title">"No profile selected"</p>
                                        <p>"Choose a language profile before starting a lesson."</p>
                                    </div>
                                }
                                    .into_any()
                            }
                            Err(e) => {
                                view! { <p class="error" role="alert">{e.to_string()}</p> }.into_any()
                            }
                        })
                }}
            </Suspense>

            <section class="section-block">
                <div class="section-heading">
                    <div>
                        <p class="section-kicker">"Workflow"</p>
                        <h2>"Run the lesson"</h2>
                    </div>
                </div>
                <ol class="workflow-list">
                    <li>
                        <span class="workflow-index">"01"</span>
                        <span><a href="/export">"Copy tutor context"</a>" and paste it into ChatGPT."</span>
                    </li>
                    <li>
                        <span class="workflow-index">"02"</span>
                        <span>"Run the lesson aloud and let the tutor adapt from live evidence."</span>
                    </li>
                    <li>
                        <span class="workflow-index">"03"</span>
                        <span>"Ask the tutor for the lesson-result block."</span>
                    </li>
                    <li>
                        <span class="workflow-index">"04"</span>
                        <span><a href="/import">"Import the result"</a>" to validate and persist it."</span>
                    </li>
                    <li>
                        <span class="workflow-index">"05"</span>
                        <span><a href="/review">"Review due cards"</a>" after the lesson."</span>
                    </li>
                </ol>
            </section>
        </Page>
    }
}

/// `/settings`: learner defaults are server-side; editing lands post-M2.
#[component]
pub fn SettingsPage() -> impl IntoView {
    view! {
        <Page>
            <header class="page-header">
                <div>
                    <p class="eyebrow">"Preferences"</p>
                    <h1>"Settings"</h1>
                </div>
            </header>
            <section class="panel settings-panel">
                <div class="settings-row">
                    <span>"Sessions / day"</span>
                    <strong>"2"</strong>
                </div>
                <div class="settings-row">
                    <span>"Session target"</span>
                    <strong>"20 min"</strong>
                </div>
                <div class="settings-row">
                    <span>"New cards"</span>
                    <strong>"8"</strong>
                </div>
                <div class="settings-row">
                    <span>"Desired retention"</span>
                    <strong>"0.90"</strong>
                </div>
                <div class="settings-row">
                    <span>"Review target"</span>
                    <strong>"12 min"</strong>
                </div>
                <p class="panel-note">"Higher desired retention increases review workload."</p>
            </section>
        </Page>
    }
}

/// `/system`: health check.
#[component]
pub fn SystemPage() -> impl IntoView {
    let health = LocalResource::new(move || async move {
        api_client::call::<_, HealthResponse>("health", &serde_json::json!({})).await
    });

    view! {
        <Page>
            <header class="page-header">
                <div>
                    <p class="eyebrow">"Runtime"</p>
                    <h1>"System"</h1>
                </div>
            </header>
            <Suspense fallback=|| view! { <p class="loading">"Checking runtime…"</p> }>
                {move || {
                    health
                        .get()
                        .map(|result| match result {
                            Ok(h) => {
                                view! {
                                    <section class="panel system-status">
                                        <div class="system-status-line">
                                            <span class="status-dot status-dot-success"></span>
                                            <span>"Local API"</span>
                                            <strong>{h.status}</strong>
                                        </div>
                                        <div class="settings-row">
                                            <span>"Version"</span>
                                            <strong>{h.version}</strong>
                                        </div>
                                    </section>
                                }
                                    .into_any()
                            }
                            Err(e) => {
                                view! { <p class="error" role="alert">{e.to_string()}</p> }.into_any()
                            }
                        })
                }}
            </Suspense>
        </Page>
    }
}
