//! Review screen (09-frontend.md §4): one card, reveal, 4 grade buttons
//! with definitions, Action submit (pending = double-submit guard),
//! keyboard 1–4 + space, immediate undo, `<Transition>` between cards.

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::ev;
use leptos::prelude::*;
use shared_schema::{ApiError, EventResultDto, QueueCardDto, QueueDto, SummaryDto};

type SubmitAction = Action<(String, u8, Option<String>), Result<EventResultDto, ApiError>>;
type UndoAction = Action<(), Result<serde_json::Value, ApiError>>;

const GRADE_LABELS: [(u8, &str); 4] = [
    (1, "1 Again — not recalled before reveal"),
    (2, "2 Hard — correct, real effort (passing!)"),
    (3, "3 Good — correct, normal effort"),
    (4, "4 Easy — immediate and confident"),
];

/// `/review`.
#[component]
pub fn ReviewPage() -> impl IntoView {
    let active = use_active_profile();

    // One session per page visit: create it, then fetch the queue.
    let session = LocalResource::new(move || {
        let id = active.id();
        async move {
            let Some(id) = id else { return Ok(None) };
            let created: serde_json::Value = api_client::post(
                "/review-sessions",
                &serde_json::json!({ "language_profile_id": id }),
            )
            .await?;
            let session_id = created["id"].as_str().unwrap_or_default().to_owned();
            let queue: QueueDto =
                api_client::get(&format!("/review-sessions/{session_id}/queue")).await?;
            Ok::<Option<QueueDto>, ApiError>(Some(queue))
        }
    });
    let session_id = Signal::derive(move || {
        session
            .get()
            .and_then(|r| r.ok().flatten())
            .map(|q| q.session_id)
            .unwrap_or_default()
    });

    let position = RwSignal::new(0_usize);
    let revealed = RwSignal::new(false);

    let submit: SubmitAction = Action::new_local(move |input: &(String, u8, Option<String>)| {
        let (card_id, grade, prior_hash) = input.clone();
        let session_id = session_id.get_untracked();
        async move {
            let op = format!("op-{card_id}-{grade}-{}", now_ms());
            let result: EventResultDto = api_client::post(
                &format!("/review-sessions/{session_id}/events"),
                &serde_json::json!({
                    "client_operation_id": op,
                    "card_id": card_id,
                    "grade": grade,
                    "presented_at": now_rfc3339(),
                    "answered_at": now_rfc3339(),
                    "response_millis": 0,
                    "answer_revealed": true,
                    "prior_state_hash": prior_hash.unwrap_or_default(),
                }),
            )
            .await?;
            position.update(|p| *p += 1);
            revealed.set(false);
            Ok(result)
        }
    });

    let undo: UndoAction = Action::new_local(move |(): &()| {
        let session_id = session_id.get_untracked();
        async move {
            let result: serde_json::Value =
                api_client::post(&format!("/review-sessions/{session_id}/undo-last"), &()).await?;
            position.update(|p| *p = p.saturating_sub(1));
            revealed.set(false);
            Ok(result)
        }
    });

    // Current card for the keyboard handler, kept in a Copy handle.
    let current_card = StoredValue::new(Option::<QueueCardDto>::None);

    // Keyboard: space = reveal, 1..=4 = grade (09 §4). Window listener is a
    // true side effect; grading goes through the same Action (pending guard).
    let _cleanup = window_event_listener(ev::keydown, move |event| {
        if submit.pending().get_untracked() {
            return;
        }
        let Some(card) = current_card.get_value() else {
            return;
        };
        match event.key().as_str() {
            " " => {
                event.prevent_default();
                revealed.set(true);
            }
            g @ ("1" | "2" | "3" | "4") if revealed.get_untracked() => {
                let grade = g.as_bytes()[0] - b'0';
                submit.dispatch((card.card_id, grade, card.prior_state_hash));
            }
            _ => {}
        }
    });

    view! {
        <Page>
            <h1>"Review"</h1>
            <Transition fallback=|| view! { <p>"Loading queue…"</p> }>
                {move || {
                    session
                        .get()
                        .map(|result| match result {
                            Ok(Some(queue)) => {
                                let total = queue.queue.len();
                                let index = position.get();
                                let card = queue.queue.get(index).cloned();
                                current_card.set_value(card.clone());
                                match card {
                                    Some(card) => {
                                        view! {
                                            <CardView
                                                card=card
                                                index=index
                                                total=total
                                                revealed=revealed
                                                submit=submit
                                                undo=undo
                                            />
                                        }
                                            .into_any()
                                    }
                                    None => {
                                        view! {
                                            <p>{format!("Session done — reviewed {index} cards.")}</p>
                                            <a href="/review/summary">"See summary"</a>
                                        }
                                            .into_any()
                                    }
                                }
                            }
                            Ok(None) => view! { <p>"Pick a language profile first."</p> }.into_any(),
                            Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                        })
                }}
            </Transition>
        </Page>
    }
}

#[component]
fn CardView(
    card: QueueCardDto,
    index: usize,
    total: usize,
    revealed: RwSignal<bool>,
    submit: SubmitAction,
    undo: UndoAction,
) -> impl IntoView {
    let handle = StoredValue::new((card.card_id.clone(), card.prior_state_hash.clone()));
    let accepted = card.accepted_answers.clone();
    view! {
        <div class="review-card">
            <p class="progress">{format!("{} / {total}", index + 1)}</p>
            <p class="intent">{card.intent.clone()}</p>
            <p class="prompt">{card.prompt.clone()}</p>
            <Show
                when=move || revealed.get()
                fallback=move || {
                    view! {
                        <button on:click=move |_| revealed.set(true)>
                            "Reveal (space) — say it out loud first"
                        </button>
                    }
                }
            >
                <div class="answer">
                    <p class="primary">{card.answer.clone()}</p>
                    {(!accepted.is_empty())
                        .then(|| {
                            view! {
                                <p class="accepted">"Also accepted: " {accepted.join(" · ")}</p>
                            }
                        })}
                    {card.explanation.clone().map(|e| view! { <p class="explanation">{e}</p> })}
                    {card
                        .pronunciation_note
                        .clone()
                        .map(|n| view! { <p class="pronunciation">{n}</p> })}
                </div>
                <div class="grades">
                    {GRADE_LABELS
                        .into_iter()
                        .map(|(grade, label)| {
                            view! {
                                <button
                                    disabled=submit.pending()
                                    on:click=move |_| {
                                        let (card_id, hash) = handle.get_value();
                                        submit.dispatch((card_id, grade, hash));
                                    }
                                >
                                    {label}
                                </button>
                            }
                        })
                        .collect::<Vec<_>>()}
                </div>
            </Show>
            <button
                class="undo"
                on:click=move |_| {
                    undo.dispatch(());
                }
            >
                "Undo last grade"
            </button>
        </div>
    }
}

/// `/review/summary`.
#[component]
pub fn SummaryPage() -> impl IntoView {
    let active = use_active_profile();
    let summary = LocalResource::new(move || {
        let id = active.id();
        async move {
            let Some(id) = id else { return Ok(None) };
            api_client::get::<SummaryDto>(&format!("/review/summary?language_profile_id={id}"))
                .await
                .map(Some)
        }
    });
    view! {
        <Page>
            <h1>"Today"</h1>
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    summary
                        .get()
                        .map(|result| match result {
                            Ok(Some(s)) => {
                                view! {
                                    <ul>
                                        <li>{format!("{} reviews", s.reviews_today)}</li>
                                        <li>
                                            {format!(
                                                "again {} · hard {} · good {} · easy {}",
                                                s.grade_distribution.again,
                                                s.grade_distribution.hard,
                                                s.grade_distribution.good,
                                                s.grade_distribution.easy,
                                            )}
                                        </li>
                                        <li>{format!("{} min", s.review_seconds_today / 60)}</li>
                                    </ul>
                                }
                                    .into_any()
                            }
                            Ok(None) => view! { <p>"No profile selected."</p> }.into_any(),
                            Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                        })
                }}
            </Suspense>
        </Page>
    }
}

fn now_rfc3339() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
}

fn now_ms() -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // ms epoch < 2^53
    let ms = js_sys::Date::now() as u64;
    ms
}
