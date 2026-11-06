//! Learning items, errors, listening pages (WBS 6.6).

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::{ApiError, ErrorTargetDto};

/// `/learning-items` + `/listening`: item inventory by status.
#[component]
pub fn ItemsPage() -> impl IntoView {
    let active = use_active_profile();
    let state = LocalResource::new(move || {
        let id = active.id();
        async move {
            let Some(id) = id else {
                return Ok::<Option<serde_json::Value>, ApiError>(None);
            };
            api_client::call::<_, serde_json::Value>(
                "get_learning_state",
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
                    <p class="eyebrow">"Inventory"</p>
                    <h1>"Learning items"</h1>
                </div>
            </header>

            <Suspense fallback=|| view! { <p class="loading">"Loading inventory…"</p> }>
                {move || {
                    state
                        .get()
                        .map(|result| match result {
                            Ok(Some(s)) => {
                                let items = s["items_by_status"]
                                    .as_object()
                                    .map(|m| {
                                        m.iter()
                                            .map(|(name, value)| {
                                                (
                                                    name.replace('_', " "),
                                                    value.as_u64().unwrap_or_default(),
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default();
                                view! {
                                    <div class="status-grid status-grid-adaptive">
                                        {items
                                            .into_iter()
                                            .map(|(name, count)| {
                                                view! {
                                                    <div>
                                                        <span>{count}</span>
                                                        <small>{name}</small>
                                                    </div>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </div>
                                }
                                    .into_any()
                            }
                            Ok(None) => {
                                view! {
                                    <div class="empty-state">
                                        <p class="empty-state-title">"No profile selected"</p>
                                        <p>"Choose a language profile to inspect its learning state."</p>
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
            api_client::call::<_, Vec<ErrorTargetDto>>(
                "list_error_targets",
                &serde_json::json!({ "language_profile_id": id }),
            )
            .await
        }
    });

    view! {
        <Page>
            <header class="page-header">
                <div>
                    <p class="eyebrow">"Adaptive signals"</p>
                    <h1>"Error targets"</h1>
                </div>
            </header>

            <Suspense fallback=|| view! { <p class="loading">"Loading error targets…"</p> }>
                {move || {
                    targets
                        .get()
                        .map(|result| match result {
                            Ok(list) if list.is_empty() => {
                                view! {
                                    <div class="empty-state">
                                        <p class="empty-state-title">"No active error targets"</p>
                                        <p>"Recurring corrections will appear here when the evidence warrants tracking."</p>
                                    </div>
                                }
                                    .into_any()
                            }
                            Ok(list) => {
                                view! {
                                    <ul class="entity-list errors">
                                        {list
                                            .iter()
                                            .map(|t| {
                                                view! {
                                                    <li class=("method-change", t.method_change_required)>
                                                        <div class="error-pair">
                                                            <span class="error-original">{t.original_production.clone()}</span>
                                                            <span class="error-arrow" aria-hidden="true">"→"</span>
                                                            <span class="error-corrected">{t.corrected_production.clone()}</span>
                                                        </div>
                                                        <div class="entity-meta-row">
                                                            <span class="badge">{t.core_category.clone()}</span>
                                                            <span class="badge">{t.status.clone()}</span>
                                                            <span class="entity-meta">
                                                                {format!("{} sightings", t.occurrence_count)}
                                                            </span>
                                                            {t.method_change_required
                                                                .then(|| {
                                                                    view! {
                                                                        <span class="method-change-label">
                                                                            "Method change"
                                                                        </span>
                                                                    }
                                                                })}
                                                        </div>
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
