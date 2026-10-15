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

/// Left-border accent per grade — a light differentiation cue, not a wall
/// of color, since review happens under time pressure (09 §4).
fn grade_class(grade: u8) -> &'static str {
    match grade {
        1 => "grade grade-again",
        2 => "grade grade-hard",
        3 => "grade grade-good",
        4 => "grade grade-easy",
        _ => "grade",
    }
}

/// `/review`.
#[component]
pub fn ReviewPage() -> impl IntoView {
    let active = use_active_profile();

    // One session per page visit: replay any offline backlog first, then
    // create the session and fetch the queue; when the network is gone,
    // fall back to the prefetched batch (09 §7).
    let offline_mode = RwSignal::new(false);
    let session = LocalResource::new(move || {
        let id = active.id();
        async move {
            let Some(id) = id else { return Ok(None) };
            let report = crate::offline::replay().await;
            if report.interrupted
                && let Some(cached) = crate::offline::cached_queue()
            {
                offline_mode.set(true);
                return Ok(Some(cached));
            }
            let created: serde_json::Value = match api_client::call(
                "create_review_session",
                &serde_json::json!({ "language_profile_id": id }),
            )
            .await
            {
                Ok(created) => created,
                Err(ApiError::Network(_)) => {
                    return crate::offline::cached_queue().map_or_else(
                        || Err(ApiError::Network("offline, no prefetched batch".to_owned())),
                        |cached| {
                            offline_mode.set(true);
                            Ok(Some(cached))
                        },
                    );
                }
                Err(e) => return Err(e),
            };
            let session_id = created["id"].as_str().unwrap_or_default().to_owned();
            let queue: QueueDto = api_client::call(
                "get_review_queue",
                &serde_json::json!({ "session_id": session_id }),
            )
            .await?;
            crate::offline::cache_queue(&queue);
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

    // Reveal instant, for measured response latency (non-reactive handle).
    let revealed_at_ms = StoredValue::new(0_u64);

    let submit: SubmitAction = Action::new_local(move |input: &(String, u8, Option<String>)| {
        let (card_id, grade, prior_hash) = input.clone();
        let session_id = session_id.get_untracked();
        async move {
            let op = format!("op-{card_id}-{grade}-{}", now_ms());
            let response_millis =
                u32::try_from(now_ms().saturating_sub(revealed_at_ms.get_value()))
                    .unwrap_or(0)
                    .min(600_000);
            let body = serde_json::json!({
                "client_operation_id": op,
                "card_id": card_id,
                "grade": grade,
                "presented_at": now_rfc3339(),
                "answered_at": now_rfc3339(),
                "response_millis": response_millis,
                "answer_revealed": true,
                "prior_state_hash": prior_hash.clone().unwrap_or_default(),
            });
            let mut params = body;
            params["session_id"] = serde_json::Value::String(session_id.clone());
            match api_client::call::<_, EventResultDto>("review_card", &params).await {
                Ok(result) => {
                    position.update(|p| *p += 1);
                    revealed.set(false);
                    Ok(result)
                }
                Err(ApiError::Network(_)) => {
                    // Offline: queue and continue locally (09 §7).
                    let queued = crate::offline::queue_event(crate::offline::PendingEvent {
                        session_id,
                        client_operation_id: op,
                        card_id: card_id.clone(),
                        grade,
                        presented_at: now_rfc3339(),
                        answered_at: now_rfc3339(),
                        response_millis,
                        prior_state_hash: prior_hash.unwrap_or_default(),
                    });
                    if !queued {
                        return Err(ApiError::Network(
                            "offline queue full — reconnect to sync".to_owned(),
                        ));
                    }
                    offline_mode.set(true);
                    position.update(|p| *p += 1);
                    revealed.set(false);
                    Ok(EventResultDto {
                        event_id: String::new(),
                        card_id,
                        card_status: None,
                        duplicate: false,
                    })
                }
                Err(e) => Err(e),
            }
        }
    });

    let undo: UndoAction = Action::new_local(move |(): &()| {
        let session_id = session_id.get_untracked();
        async move {
            let result: serde_json::Value = api_client::call(
                "undo_review",
                &serde_json::json!({ "session_id": session_id }),
            )
            .await?;
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
                revealed_at_ms.set_value(now_ms());
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
                                                revealed_at_ms=revealed_at_ms
                                                submit=submit
                                                undo=undo
                                            />
                                        }
                                            .into_any()
                                    }
                                    None => {
                                        view! {
                                            <p>{format!("Session done — reviewed {index} cards.")}</p>
                                            <div class="actions">
                                                <a href="/review/summary">"See summary"</a>
                                                // Undo stays reachable after the last grade (09 §4).
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
                                            .into_any()
                                    }
                                }
                            }
                            Ok(None) => view! { <p>"Pick a language profile first."</p> }.into_any(),
                            Err(e) => {
                                view! { <p class="error" role="alert">{e.to_string()}</p> }.into_any()
                            }
                        })
                }}
            </Transition>
            {move || {
                offline_mode
                    .get()
                    .then(|| {
                        view! {
                            <p class="offline" role="status">
                                "Offline — grades sync on reconnect."
                            </p>
                        }
                    })
            }}
        </Page>
    }
}

#[component]
fn CardView(
    card: QueueCardDto,
    index: usize,
    total: usize,
    revealed: RwSignal<bool>,
    revealed_at_ms: StoredValue<u64>,
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
                        <button
                            class="btn-primary"
                            on:click=move |_| {
                                revealed_at_ms.set_value(now_ms());
                                revealed.set(true);
                            }
                        >
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
                                    class=grade_class(grade)
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
            api_client::call::<_, SummaryDto>(
                "get_review_summary",
                &serde_json::json!({ "language_profile_id": id }),
            )
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
                            Err(e) => {
                                view! { <p class="error" role="alert">{e.to_string()}</p> }.into_any()
                            }
                        })
                }}
            </Suspense>
        </Page>
    }
}

/// `/stats`: retention weeks + due forecast, rendered as native inline SVG
/// bars — no JS chart library (09-frontend.md §1).
#[component]
pub fn StatsPage() -> impl IntoView {
    let active = use_active_profile();
    let stats = LocalResource::new(move || {
        let id = active.id();
        async move {
            let Some(id) = id else { return Ok(None) };
            let retention = api_client::call::<_, serde_json::Value>(
                "get_review_retention",
                &serde_json::json!({ "language_profile_id": id, "days": 28 }),
            )
            .await?;
            let forecast = api_client::call::<_, serde_json::Value>(
                "get_review_forecast",
                &serde_json::json!({ "language_profile_id": id, "days": 7 }),
            )
            .await?;
            Ok(Some((retention, forecast)))
        }
    });
    view! {
        <Page>
            <h1>"Stats"</h1>
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    stats
                        .get()
                        .map(|result: Result<_, ApiError>| match result {
                            Ok(Some((retention, forecast))) => {
                                let overall = retention["retention"].as_f64();
                                let weeks: Vec<_> = retention["weeks"]
                                    .as_array()
                                    .cloned()
                                    .unwrap_or_default();
                                let due: Vec<u64> = forecast["due_per_day"]
                                    .as_array()
                                    .map(|a| a.iter().filter_map(serde_json::Value::as_u64).collect())
                                    .unwrap_or_default();
                                let max_due = due.iter().copied().max().unwrap_or(0).max(1);
                                view! {
                                    <h2>"Retention (28 days)"</h2>
                                    <p>
                                        {overall
                                            .map_or_else(
                                                || "No reviews yet.".to_owned(),
                                                |r| {
                                                    format!(
                                                        "{:.0}% over {} reviews",
                                                        r * 100.0,
                                                        retention["reviews"].as_u64().unwrap_or(0),
                                                    )
                                                },
                                            )}
                                    </p>
                                    <ul class="stats-weeks">
                                        {weeks
                                            .iter()
                                            .map(|week| {
                                                let n = week["reviews"].as_u64().unwrap_or(0);
                                                let rate = week["retention"].as_f64();
                                                let label = week["week_start"]
                                                    .as_str()
                                                    .map(|s| s.chars().take(10).collect::<String>())
                                                    .unwrap_or_default();
                                                view! {
                                                    <li class="stat-row">
                                                        <span class="stat-label">{label}</span>
                                                        {match rate {
                                                            Some(rate) => {
                                                                view! {
                                                                    <div class="stat-bar-wrap">
                                                                        {bar_svg(rate)}
                                                                        <span class="stat-value">
                                                                            {format!("{:.0}% ({n})", rate * 100.0)}
                                                                        </span>
                                                                    </div>
                                                                }
                                                                    .into_any()
                                                            }
                                                            None => {
                                                                view! { <span class="stat-value">"— (0)"</span> }
                                                                    .into_any()
                                                            }
                                                        }}
                                                    </li>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </ul>
                                    <h2>"Due next 7 days"</h2>
                                    <ul class="stats-forecast">
                                        {due
                                            .iter()
                                            .enumerate()
                                            .map(|(day, count)| {
                                                #[allow(clippy::cast_precision_loss)]
                                                let fraction = *count as f64 / max_due as f64;
                                                view! {
                                                    <li class="stat-row">
                                                        <span class="stat-label">{format!("+{day}d")}</span>
                                                        <div class="stat-bar-wrap">
                                                            {bar_svg(fraction)}
                                                            <span class="stat-value">{count.to_string()}</span>
                                                        </div>
                                                    </li>
                                                }
                                            })
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

/// One inline SVG bar, `fraction` (0.0–1.0) of the track filled — a native
/// element, not a chart library (09-frontend.md §1).
fn bar_svg(fraction: f64) -> impl IntoView {
    let width = format!("{}", fraction.clamp(0.0, 1.0) * 100.0);
    view! {
        <svg class="bar" viewBox="0 0 100 10" preserveAspectRatio="none" aria-hidden="true">
            <rect class="bar-track" x="0" y="0" width="100" height="10" rx="2" />
            <rect class="bar-fill" x="0" y="0" width=width height="10" rx="2" />
        </svg>
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
