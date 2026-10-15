//! Card management (09-frontend.md §5): list + filters, detail,
//! suspend/restore/retire.

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use shared_schema::{ApiError, CardDto};

/// `/cards` (+ `/cards/redesign` shares the list until WBS 8).
#[component]
pub fn CardsPage() -> impl IntoView {
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
            <h1>"Cards"</h1>
            <label class="visually-hidden" for="cards-status-filter">
                "Filter by status"
            </label>
            <select
                id="cards-status-filter"
                on:change=move |ev| status_filter.set(event_target_value(&ev))
            >
                <option value="">"all statuses"</option>
                <option value="candidate">"candidate"</option>
                <option value="new">"new"</option>
                <option value="learning">"learning"</option>
                <option value="review">"review"</option>
                <option value="relearning">"relearning"</option>
                <option value="suspended">"suspended"</option>
                <option value="retired">"retired"</option>
            </select>
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    cards
                        .get()
                        .map(|result| match result {
                            Ok(list) => {
                                view! {
                                    <ul class="cards">
                                        {list
                                            .iter()
                                            .map(|c| {
                                                view! {
                                                    <li>
                                                        <a href=format!("/cards/{}", c.id)>
                                                            <span class="badge">{c.status.to_string()}</span>
                                                            <span class="badge">{c.intent.to_string()}</span>
                                                            <span>{c.prompt.clone()}</span>
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
pub fn CardDetailPage() -> impl IntoView {
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
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    card.get()
                        .map(|result| match result {
                            Ok(c) => {
                                view! {
                                    <div class="card-detail">
                                        <h1>{c.prompt.clone()}</h1>
                                        <p>"Answer: " {c.answer.clone()}</p>
                                        {(!c.accepted_answers.is_empty())
                                            .then(|| {
                                                view! {
                                                    <p>"Accepted: " {c.accepted_answers.join(" · ")}</p>
                                                }
                                            })}
                                        <p>{format!("intent {} · status {}", c.intent, c.status)}</p>
                                        <p>"Why: " {c.creation_reason.clone()}</p>
                                        {c.explanation.clone().map(|e| view! { <p>{e}</p> })}
                                        {c.suspended_reason
                                            .clone()
                                            .map(|r| view! { <p>"Suspended: " {r}</p> })}
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
