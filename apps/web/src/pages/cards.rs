//! Card management (09-frontend.md §5): list + filters, detail,
//! suspend/restore/retire.

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use shared_schema::{ApiError, CardDto};

/// `/cards`: card library.
#[component]
pub(crate) fn CardsPage() -> impl IntoView {
    let active = use_active_profile();
    let status_filter = RwSignal::new(String::new());
    let cards = LocalResource::new(move || {
        let id = active.id();
        let status = status_filter.get();
        async move {
            let Some(id) = id else {
                return Ok::<Vec<CardDto>, ApiError>(Vec::new());
            };
            api_client::call::<_, Vec<CardDto>>(
                "list_cards",
                &serde_json::json!({
                    "language_profile_id": id,
                    "status": (!status.is_empty()).then_some(status),
                }),
            )
            .await
        }
    });

    view! {
        <Page>
            <header class="page-header page-header-row">
                <div>
                    <p class="eyebrow">"Review library"</p>
                    <h1>"Library"</h1>
                </div>
                <div class="filter-bar">
                    <label class="visually-hidden" for="cards-status-filter">
                        "Filter by status"
                    </label>
                    <select
                        id="cards-status-filter"
                        aria-label="Filter cards by status"
                        on:change=move |ev| status_filter.set(event_target_value(&ev))
                    >
                        <option value="">"All statuses"</option>
                        <option value="candidate">"Candidate"</option>
                        <option value="new">"New"</option>
                        <option value="learning">"Learning"</option>
                        <option value="review">"Review"</option>
                        <option value="relearning">"Relearning"</option>
                        <option value="suspended">"Suspended"</option>
                        <option value="retired">"Retired"</option>
                    </select>
                </div>
            </header>

            <Suspense fallback=|| view! { <p class="loading">"Loading cards…"</p> }>
                {move || {
                    cards
                        .get()
                        .map(|result| match result {
                            Ok(list) if list.is_empty() => {
                                view! {
                                    <div class="empty-state">
                                        <p class="empty-state-title">"No cards in this view"</p>
                                        <p>"Cards appear when lesson evidence creates a useful review target."</p>
                                    </div>
                                }
                                    .into_any()
                            }
                            Ok(list) => {
                                view! {
                                    <ul class="entity-list card-list">
                                        {list
                                            .iter()
                                            .map(|c| {
                                                view! {
                                                    <li class="card-row">
                                                        <a href=format!("/cards/{}", c.id)>
                                                            <div class="card-row-copy">
                                                                <p class="card-row-prompt">{c.prompt.clone()}</p>
                                                                <div class="entity-meta-row">
                                                                    <span class="badge">{c.intent.to_string()}</span>
                                                                    <span class="badge">{c.status.to_string()}</span>
                                                                    <span class="entity-meta">
                                                                        {format!("v{}", c.content_version)}
                                                                    </span>
                                                                </div>
                                                            </div>
                                                            <span class="row-chevron" aria-hidden="true">"›"</span>
                                                        </a>
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

/// `/cards/:id`.
#[component]
pub(crate) fn CardDetailPage() -> impl IntoView {
    let params = use_params_map();
    let card_id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    let refresh = RwSignal::new(0_u32);
    let card = LocalResource::new(move || {
        let id = card_id.get();
        refresh.track();
        async move {
            api_client::call::<_, CardDto>("get_card", &serde_json::json!({ "card_id": id })).await
        }
    });

    let status_action = Action::new_local(move |verb: &&'static str| {
        let id = card_id.get_untracked();
        let verb = *verb;
        async move {
            let status = match verb {
                "suspend" => "suspended",
                "restore" => "review",
                "retire" => "retired",
                _ => verb,
            };
            let result: Result<serde_json::Value, ApiError> = api_client::call(
                "set_card_status",
                &serde_json::json!({ "card_id": id, "status": status }),
            )
            .await;
            refresh.update(|r| *r += 1);
            result
        }
    });

    view! {
        <Page>
            <Suspense fallback=|| view! { <p class="loading">"Loading card…"</p> }>
                {move || {
                    card.get()
                        .map(|result| match result {
                            Ok(c) => {
                                view! {
                                    <header class="page-header">
                                        <div class="breadcrumb">
                                            <a href="/cards">"Library"</a>
                                            <span aria-hidden="true">"›"</span>
                                            <span>"Detail"</span>
                                        </div>
                                        <div class="entity-meta-row">
                                            <span class="badge">{c.intent.to_string()}</span>
                                            <span class="badge">{c.status.to_string()}</span>
                                            <span class="entity-meta">{format!("Version {}", c.content_version)}</span>
                                        </div>
                                    </header>

                                    <div class="card-detail">
                                        <section class="study-surface">
                                            <span class="section-kicker">"Prompt"</span>
                                            <h1 class="study-prompt">{c.prompt.clone()}</h1>
                                            <div class="study-answer">
                                                <span class="section-kicker">"Preferred answer"</span>
                                                <p>{c.answer.clone()}</p>
                                            </div>
                                            {(!c.accepted_answers.is_empty())
                                                .then(|| {
                                                    view! {
                                                        <div class="detail-block">
                                                            <span class="section-kicker">"Also accepted"</span>
                                                            <p>{c.accepted_answers.join(" · ")}</p>
                                                        </div>
                                                    }
                                                })}
                                        </section>

                                        <section class="detail-grid">
                                            <div class="detail-block">
                                                <span class="section-kicker">"Why this exists"</span>
                                                <p>{c.creation_reason.clone()}</p>
                                            </div>
                                            {c.explanation
                                                .clone()
                                                .map(|e| {
                                                    view! {
                                                        <div class="detail-block">
                                                            <span class="section-kicker">"Explanation"</span>
                                                            <p>{e}</p>
                                                        </div>
                                                    }
                                                })}
                                            {c.suspended_reason
                                                .clone()
                                                .map(|reason| {
                                                    view! {
                                                        <div class="detail-block">
                                                            <span class="section-kicker">"Suspension reason"</span>
                                                            <p>{reason}</p>
                                                        </div>
                                                    }
                                                })}
                                        </section>

                                        <div class="actions">
                                            <button
                                                disabled=status_action.pending()
                                                on:click=move |_| {
                                                    status_action.dispatch("suspend");
                                                }
                                            >
                                                "Suspend"
                                            </button>
                                            <button
                                                disabled=status_action.pending()
                                                on:click=move |_| {
                                                    status_action.dispatch("restore");
                                                }
                                            >
                                                "Restore"
                                            </button>
                                            <button
                                                class="destructive"
                                                disabled=status_action.pending()
                                                on:click=move |_| {
                                                    status_action.dispatch("retire");
                                                }
                                            >
                                                "Retire"
                                            </button>
                                        </div>
                                        {move || {
                                            status_action
                                                .value()
                                                .get()
                                                .and_then(|r| r.err())
                                                .map(|e| {
                                                    view! {
                                                        <p class="error" role="alert">{e.to_string()}</p>
                                                    }
                                                })
                                        }}
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
