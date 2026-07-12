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
            api_client::get::<serde_json::Value>(&format!("/next-lesson?language_profile_id={id}"))
                .await
                .map(Some)
        }
    });
    view! {
        <Page>
            <h1>"Next lesson"</h1>
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    next.get()
                        .map(|result| match result {
                            Ok(Some(n)) => {
                                view! {
                                    <div>
                                        <p>{format!("Lesson #{}", n["lesson_number"])}</p>
                                        {n["goal"]
                                            .as_str()
                                            .map(|g| view! { <p>"Goal: " {g.to_owned()}</p> })}
                                    </div>
                                }
                                    .into_any()
                            }
                            Ok(None) => view! { <p>"No profile selected."</p> }.into_any(),
                            Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                        })
                }}
            </Suspense>
            <ol>
                <li><a href="/export">"Copy tutor context"</a>" and paste it into ChatGPT."</li>
                <li>"Run the lesson: speak out loud, let the tutor correct you."</li>
                <li>"Ask the tutor for the lesson-result block."</li>
                <li><a href="/import">"Paste it into Import"</a>" → validate → apply."</li>
                <li><a href="/review">"Review your due cards."</a></li>
            </ol>
        </Page>
    }
}

/// `/settings`: learner defaults are server-side; editing lands post-M2.
#[component]
pub fn SettingsPage() -> impl IntoView {
    view! {
        <Page>
            <h1>"Settings"</h1>
            <p>"Defaults: 2 sessions/day · 20 min · 8 new cards · retention 0.90 · 12 review min."</p>
            <p>"Raising desired retention raises workload (05-review-engine.md §6)."</p>
        </Page>
    }
}

/// `/system`: health check.
#[component]
pub fn SystemPage() -> impl IntoView {
    let health =
        LocalResource::new(
            move || async move { api_client::get::<HealthResponse>("/health").await },
        );
    view! {
        <Page>
            <h1>"System"</h1>
            <Suspense fallback=|| view! { <p>"Checking…"</p> }>
                {move || {
                    health
                        .get()
                        .map(|result| match result {
                            Ok(h) => {
                                view! { <p>{format!("API {} — v{}", h.status, h.version)}</p> }
                                    .into_any()
                            }
                            Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                        })
                }}
            </Suspense>
        </Page>
    }
}
