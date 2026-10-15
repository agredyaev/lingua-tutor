//! Dashboard (09-frontend.md §6): one aggregated fetch, exceptions first.

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::DashboardDto;

/// `/dashboard`.
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
            <h1>"Dashboard"</h1>
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    dashboard
                        .get()
                        .map(|result| match result {
                            Ok(Some(d)) => view! { <DashboardView d=d /> }.into_any(),
                            Ok(None) => {
                                view! {
                                    <p>"No language profile yet — create one under "
                                        <a href="/languages">"Languages"</a>"."</p>
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
        <section>
            {(!d.exceptions.is_empty())
                .then(|| {
                    view! {
                        <div class="callout-warning">
                            <h2>"Needs attention"</h2>
                            <ul>
                                {d.exceptions
                                    .iter()
                                    .map(|e| view! { <li>{e.clone()}</li> })
                                    .collect::<Vec<_>>()}
                            </ul>
                        </div>
                    }
                })}
            <ul class="stats">
                <li><a href="/review">{format!("{} cards due", d.due_cards)}</a></li>
                <li>{format!("{} reviews today ({} min)", d.reviews_today, d.review_seconds_today / 60)}</li>
                <li>{format!("new-card budget left: {}", d.new_card_budget)}</li>
                <li><a href="/errors">{format!("{} active error targets", d.active_error_targets)}</a></li>
                <li>{format!("{} rechecks due", d.due_error_rechecks)}</li>
                <li>
                    <a href="/learn">
                        {format!("next lesson #{}", d.next_lesson_number)}
                        {d.next_lesson_goal.as_ref().map(|g| format!(" — {g}")).unwrap_or_default()}
                    </a>
                </li>
            </ul>
            {(!d.open_recommendations.is_empty())
                .then(|| {
                    view! {
                        <div class="callout-info">
                            <h2>"Tutor suggestions"</h2>
                            <ul>
                                {d.open_recommendations
                                    .iter()
                                    .map(|r| view! { <li>{r.clone()}</li> })
                                    .collect::<Vec<_>>()}
                            </ul>
                        </div>
                    }
                })}
            <h2>"Items"</h2>
            <ul>
                {d.items_by_status
                    .iter()
                    .map(|(status, count)| view! { <li>{format!("{status}: {count}")}</li> })
                    .collect::<Vec<_>>()}
            </ul>
        </section>
    }
}
