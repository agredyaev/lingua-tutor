//! Learning items, errors, listening pages (WBS 6.6).

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::{ApiError, ErrorTargetDto};

/// `/learning-items` + `/listening`: item inventory by status
/// (dedicated listening stats land with analytics, WBS 8).
#[component]
pub fn ItemsPage() -> impl IntoView {
    let active = use_active_profile();
    let state = LocalResource::new(move || {
        let id = active.id();
        async move {
            let Some(id) = id else {
                return Ok::<Option<serde_json::Value>, ApiError>(None);
            };
            api_client::get::<serde_json::Value>(&format!(
                "/learning-state?language_profile_id={id}"
            ))
            .await
            .map(Some)
        }
    });
    view! {
        <Page>
            <h1>"Learning items"</h1>
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    state
                        .get()
                        .map(|result| match result {
                            Ok(Some(s)) => {
                                let items = s["items_by_status"]
                                    .as_object()
                                    .map(|m| {
                                        m.iter()
                                            .map(|(k, v)| format!("{k}: {v}"))
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default();
                                view! {
                                    <ul>
                                        {items
                                            .into_iter()
                                            .map(|line| view! { <li>{line}</li> })
                                            .collect::<Vec<_>>()}
                                    </ul>
                                }
                                    .into_any()
                            }
                            Ok(None) => view! { <p>"No profile selected."</p> }.into_any(),
                            Err(e) => {
                                view! { <p class="error" role="alert">{e.to_string()}</p> }.into_any()
                            }
                        })
                }}
            </Suspense>
        </Page>
    }
}

/// `/errors`: active error targets, method-change flags first.
#[component]
pub fn ErrorsPage() -> impl IntoView {
    let active = use_active_profile();
    let targets = LocalResource::new(move || {
        let id = active.id();
        async move {
            let Some(id) = id else {
                return Ok::<Vec<ErrorTargetDto>, ApiError>(Vec::new());
            };
            api_client::get::<Vec<ErrorTargetDto>>(&format!(
                "/error-targets?language_profile_id={id}"
            ))
            .await
        }
    });
    view! {
        <Page>
            <h1>"Error targets"</h1>
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    targets
                        .get()
                        .map(|result| match result {
                            Ok(list) => {
                                view! {
                                    <ul class="errors">
                                        {list
                                            .iter()
                                            .map(|t| {
                                                view! {
                                                    <li class=("method-change", t.method_change_required)>
                                                        <strong>
                                                            {format!(
                                                                "{} → {}",
                                                                t.original_production,
                                                                t.corrected_production,
                                                            )}
                                                        </strong>
                                                        <p>
                                                            {format!(
                                                                "{} · {} sightings · {}{}",
                                                                t.core_category,
                                                                t.occurrence_count,
                                                                t.status,
                                                                if t.method_change_required {
                                                                    " · METHOD CHANGE"
                                                                } else {
                                                                    ""
                                                                },
                                                            )}
                                                        </p>
                                                    </li>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </ul>
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
