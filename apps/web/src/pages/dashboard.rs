//! Learner home: current review pressure, progress and next tutor focus.

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::DashboardDto;

/// Learner home.
#[component]
pub fn DashboardPage() -> impl IntoView {
    let active = use_active_profile();
    let dashboard = LocalResource::new(move || {
        let id = active.id();
        async move {
            match id {
                Some(id) => api_client::call::<_, DashboardDto>(
                    "get_dashboard",
                    &serde_json::json!({ "language_profile_id": id }),
                )
                .await
                .map(Some),
                None => Ok(None),
            }
        }
    });

    view! {
        <Page>
            <header class="page-header">
                <h1>"Home"</h1>
            </header>

            <Suspense fallback=|| view! { <p class="loading">"Loading…"</p> }>
                {move || {
                    dashboard
                        .get()
                        .map(|result| match result {
                            Ok(Some(d)) => view! { <DashboardView d=d /> }.into_any(),
                            Ok(None) => {
                                view! {
                                    <div class="empty-state">
                                        <p class="empty-state-title">"Choose a language to begin"</p>
                                        <p>
                                            <a href="/languages">"Create or select a language profile"</a>
                                            " and the tutor will build your learning state automatically."
                                        </p>
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
        </Page>
    }
}

#[component]
fn DashboardView(d: DashboardDto) -> impl IntoView {
    view! {
        <section class="dashboard-stack">
            {(!d.exceptions.is_empty())
                .then(|| {
                    view! {
                        <div class="callout-warning">
                            <div class="callout-heading">
                                <span class="status-dot status-dot-warning"></span>
                                <h2>"Needs attention"</h2>
                            </div>
                            <ul class="compact-list">
                                {d.exceptions
                                    .iter()
                                    .map(|e| view! { <li>{e.clone()}</li> })
                                    .collect::<Vec<_>>()}
                            </ul>
                        </div>
                    }
                })}

            <div class="metric-grid learner-metric-grid">
                <a class="metric-card metric-card-action" href="/review">
                    <span class="metric-value">{d.due_cards}</span>
                    <span class="metric-label">"Due for review"</span>
                </a>
                <div class="metric-card">
                    <span class="metric-value">{d.reviews_today}</span>
                    <span class="metric-label">
                        {format!("Reviewed today · {} min", d.review_seconds_today / 60)}
                    </span>
                </div>
                <div class="metric-card">
                    <span class="metric-value">{d.active_error_targets}</span>
                    <span class="metric-label">"Current focus areas"</span>
                </div>
                <div class="metric-card metric-card-wide">
                    <span class="metric-value">{format!("#{}", d.next_lesson_number)}</span>
                    <span class="metric-label">"Next lesson"</span>
                    {d.next_lesson_goal
                        .as_ref()
                        .map(|goal| view! { <span class="metric-detail">{goal.clone()}</span> })}
                </div>
            </div>

            {(!d.open_recommendations.is_empty())
                .then(|| {
                    view! {
                        <section class="section-block">
                            <div class="section-heading">
                                <h2>"Tutor focus"</h2>
                            </div>
                            <ul class="learner-focus-list">
                                {d.open_recommendations
                                    .iter()
                                    .map(|r| view! { <li>{r.clone()}</li> })
                                    .collect::<Vec<_>>()}
                            </ul>
                        </section>
                    }
                })}
        </section>
    }
}
