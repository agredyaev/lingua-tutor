//! Import/export pages (09-frontend.md §8): the primary no-MCP loop.

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::{ApplyResponseDto, ValidateResponseDto};

/// `/import`: paste → validate → preview → apply with validation token.
#[component]
pub fn ImportPage() -> impl IntoView {
    let pasted = RwSignal::new(String::new());
    let validated = RwSignal::new(Option::<ValidateResponseDto>::None);
    let applied = RwSignal::new(Option::<ApplyResponseDto>::None);
    let error = RwSignal::new(Option::<String>::None);

    let validate = Action::new_local(move |body: &String| {
        let body = body.clone();
        async move {
            error.set(None);
            applied.set(None);
            match api_client::post_text::<ValidateResponseDto>(
                "/import/lesson-result/validate",
                body,
            )
            .await
            {
                Ok(response) => validated.set(Some(response)),
                Err(e) => {
                    validated.set(None);
                    error.set(Some(e.to_string()));
                }
            }
        }
    });

    let apply = Action::new_local(move |(): &()| {
        let body = pasted.get_untracked();
        let token = validated
            .get_untracked()
            .map(|v| v.validation_token)
            .unwrap_or_default();
        async move {
            error.set(None);
            match api_client::post_text::<ApplyResponseDto>(
                &format!("/import/lesson-result/apply?validation_token={token}"),
                body,
            )
            .await
            {
                Ok(response) => {
                    applied.set(Some(response));
                    validated.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        }
    });

    view! {
        <Page>
            <h1>"Import lesson result"</h1>
            <p>"Paste the tutor's lesson-result JSON block:"</p>
            <textarea
                rows=12
                prop:value=move || pasted.get()
                on:input=move |ev| pasted.set(event_target_value(&ev))
            ></textarea>
            <button
                disabled=validate.pending()
                on:click=move |_| {
                    validate.dispatch(pasted.get_untracked());
                }
            >
                "Validate"
            </button>
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
            {move || {
                validated
                    .get()
                    .map(|v| {
                        let p = v.preview;
                        view! {
                            <div class="preview">
                                <h2>"Preview — " {p.lesson_scenario.clone()}</h2>
                                <ul>
                                    <li>{format!("{} new items", p.new_learning_items)}</li>
                                    <li>{format!("{} evidence entries", p.evidence)}</li>
                                    <li>{format!("{} error occurrences", p.error_occurrences)}</li>
                                    <li>{format!("{} listening attempts", p.listening_attempts)}</li>
                                    <li class=(
                                        "destructive",
                                        p.card_mutations > 0,
                                    )>{format!("{} card mutations", p.card_mutations)}</li>
                                </ul>
                                <button
                                    disabled=apply.pending()
                                    on:click=move |_| {
                                        apply.dispatch(());
                                    }
                                >
                                    "Apply"
                                </button>
                            </div>
                        }
                    })
            }}
            {move || {
                applied
                    .get()
                    .map(|a| {
                        view! {
                            <div class="applied">
                                <h2>
                                    {a
                                        .status
                                        .as_deref()
                                        .map_or("Applied", |_| "Duplicate — nothing written")}
                                </h2>
                                <ul>
                                    {a
                                        .card_mutations
                                        .iter()
                                        .map(|m| {
                                            view! {
                                                <li>
                                                    {format!(
                                                        "{}: {}{}",
                                                        m.client_operation_id,
                                                        m.verdict,
                                                        m.note.as_ref().map(|n| format!(" — {n}")).unwrap_or_default(),
                                                    )}
                                                </li>
                                            }
                                        })
                                        .collect::<Vec<_>>()}
                                </ul>
                                {(!a.warnings.is_empty())
                                    .then(|| {
                                        view! {
                                            <ul class="warnings">
                                                {a
                                                    .warnings
                                                    .iter()
                                                    .map(|w| view! { <li>{w.clone()}</li> })
                                                    .collect::<Vec<_>>()}
                                            </ul>
                                        }
                                    })}
                            </div>
                        }
                    })
            }}
        </Page>
    }
}

/// `/export`: tutor context (primary flow, copy button), full JSON, CSV.
#[component]
pub fn ExportPage() -> impl IntoView {
    let active = use_active_profile();
    let context_text = RwSignal::new(Option::<String>::None);
    let copied = RwSignal::new(false);

    let fetch_context = Action::new_local(move |(): &()| {
        let id = active.id();
        async move {
            let Some(id) = id else { return };
            match api_client::get_text(&format!("/export/tutor-context?language_profile_id={id}"))
                .await
            {
                Ok(text) => context_text.set(Some(text)),
                Err(e) => context_text.set(Some(format!("error: {e}"))),
            }
        }
    });

    let copy = Action::new_local(move |(): &()| async move {
        if let Some(text) = context_text.get_untracked() {
            let promise = window().navigator().clipboard().write_text(&text);
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            copied.set(true);
        }
    });

    view! {
        <Page>
            <h1>"Export"</h1>
            <section>
                <h2>"Tutor context (paste into ChatGPT)"</h2>
                <button
                    disabled=fetch_context.pending()
                    on:click=move |_| {
                        copied.set(false);
                        fetch_context.dispatch(());
                    }
                >
                    "Fetch tutor context"
                </button>
                {move || {
                    context_text
                        .get()
                        .map(|text| {
                            view! {
                                <div>
                                    <button on:click=move |_| {
                                        copy.dispatch(());
                                    }>{move || if copied.get() { "Copied ✓" } else { "Copy" }}</button>
                                    <pre class="context">{text}</pre>
                                </div>
                            }
                        })
                }}
            </section>
            <section>
                <h2>"Data export"</h2>
                {move || {
                    active
                        .id()
                        .map(|id| {
                            view! {
                                <ul>
                                    <li>
                                        <a href=format!(
                                            "/api/v1/export/full?language_profile_id={id}",
                                        )>"Full JSON export"</a>
                                    </li>
                                    <li>
                                        <a href=format!(
                                            "/api/v1/export/cards.csv?language_profile_id={id}",
                                        )>"cards.csv"</a>
                                    </li>
                                </ul>
                            }
                        })
                }}
            </section>
        </Page>
    }
}
