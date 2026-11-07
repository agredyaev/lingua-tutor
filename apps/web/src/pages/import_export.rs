//! Import/export pages (09-frontend.md §8): the primary no-MCP loop.

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::{ApplyResponseDto, ValidateResponseDto};

/// `/import`: paste → validate → preview → apply with validation token.
#[component]
pub(crate) fn ImportPage() -> impl IntoView {
    let pasted = RwSignal::new(String::new());
    let validated = RwSignal::new(Option::<ValidateResponseDto>::None);
    let applied = RwSignal::new(Option::<ApplyResponseDto>::None);
    let error = RwSignal::new(Option::<String>::None);

    let validate = Action::new_local(move |body: &String| {
        let body = body.clone();
        async move {
            error.set(None);
            applied.set(None);
            match api_client::call::<_, ValidateResponseDto>(
                "validate_lesson_result",
                &serde_json::json!({ "text": body }),
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
            match api_client::call::<_, ApplyResponseDto>(
                "apply_lesson_result",
                &serde_json::json!({ "text": body, "validation_token": token }),
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
            <header class="page-header">
                <div>
                    <p class="eyebrow">"Tutor handoff"</p>
                    <h1>"Import lesson result"</h1>
                    <p class="page-description">
                        "Validate the tutor result before anything is written. The preview keeps persistence explicit."
                    </p>
                </div>
            </header>

            <section class="panel import-panel">
                <label class="field" for="import-paste">
                    <span class="field-label">"Lesson-result JSON"</span>
                    <span class="field-hint">"Paste the single structured block returned by the tutor."</span>
                </label>
                <textarea
                    id="import-paste"
                    rows=12
                    prop:value=move || pasted.get()
                    on:input=move |ev| pasted.set(event_target_value(&ev))
                ></textarea>
                <div class="form-actions">
                    <button
                        class="btn-primary"
                        disabled=validate.pending()
                        on:click=move |_| {
                            validate.dispatch(pasted.get_untracked());
                        }
                    >
                        "Validate result"
                    </button>
                </div>
            </section>

            {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}

            {move || {
                validated
                    .get()
                    .map(|v| {
                        let p = v.preview;
                        view! {
                            <section class="preview">
                                <div class="section-heading">
                                    <div>
                                        <p class="section-kicker">"Validated preview"</p>
                                        <h2>{p.lesson_scenario.clone()}</h2>
                                    </div>
                                </div>
                                <div class="preview-grid">
                                    <div><strong>{p.new_learning_items}</strong><span>"New items"</span></div>
                                    <div><strong>{p.evidence}</strong><span>"Evidence"</span></div>
                                    <div><strong>{p.error_occurrences}</strong><span>"Errors"</span></div>
                                    <div><strong>{p.listening_attempts}</strong><span>"Listening"</span></div>
                                    <div class=("destructive", p.card_mutations > 0)>
                                        <strong>{p.card_mutations}</strong><span>"Card mutations"</span>
                                    </div>
                                </div>
                                <div class="form-actions">
                                    <button
                                        class="btn-primary"
                                        disabled=apply.pending()
                                        on:click=move |_| {
                                            apply.dispatch(());
                                        }
                                    >
                                        "Apply to learner state"
                                    </button>
                                </div>
                            </section>
                        }
                    })
            }}

            {move || {
                applied
                    .get()
                    .map(|a| {
                        view! {
                            <section class="applied">
                                <div class="section-heading">
                                    <div>
                                        <p class="section-kicker">"Result"</p>
                                        <h2>
                                            {a
                                                .status
                                                .as_deref()
                                                .map_or("Applied", |_| "Duplicate — nothing written")}
                                        </h2>
                                    </div>
                                </div>

                                {(!a.card_mutations.is_empty())
                                    .then(|| {
                                        view! {
                                            <ul class="entity-list mutation-list">
                                                {a
                                                    .card_mutations
                                                    .iter()
                                                    .map(|m| {
                                                        view! {
                                                            <li>
                                                                <span class="badge">{m.verdict.clone()}</span>
                                                                <div>
                                                                    <p>{m.client_operation_id.clone()}</p>
                                                                    {m.note
                                                                        .clone()
                                                                        .map(|note| view! { <p class="entity-meta">{note}</p> })}
                                                                </div>
                                                            </li>
                                                        }
                                                    })
                                                    .collect::<Vec<_>>()}
                                            </ul>
                                        }
                                    })}

                                {(!a.warnings.is_empty())
                                    .then(|| {
                                        view! {
                                            <div class="callout-warning">
                                                <div class="callout-heading">
                                                    <span class="status-dot status-dot-warning"></span>
                                                    <h2>"Warnings"</h2>
                                                </div>
                                                <ul class="compact-list">
                                                    {a
                                                        .warnings
                                                        .iter()
                                                        .map(|w| view! { <li>{w.clone()}</li> })
                                                        .collect::<Vec<_>>()}
                                                </ul>
                                            </div>
                                        }
                                    })}
                            </section>
                        }
                    })
            }}
        </Page>
    }
}

/// `/export`: tutor context (primary flow, copy button), full JSON, CSV.
#[component]
pub(crate) fn ExportPage() -> impl IntoView {
    let active = use_active_profile();
    let context_text = RwSignal::new(Option::<String>::None);
    let copied = RwSignal::new(false);

    let fetch_context = Action::new_local(move |(): &()| {
        let id = active.id();
        async move {
            let Some(id) = id else { return };
            match api_client::call::<_, serde_json::Value>(
                "get_tutor_context",
                &serde_json::json!({ "language_profile_id": id }),
            )
            .await
            {
                Ok(value) => {
                    let text =
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
                    context_text.set(Some(text));
                }
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
            <header class="page-header">
                <div>
                    <p class="eyebrow">"Tutor handoff"</p>
                    <h1>"Export"</h1>
                    <p class="page-description">
                        "Fetch the current tutor context or export the persisted learner data."
                    </p>
                </div>
            </header>

            <section class="panel export-panel">
                <div class="section-heading">
                    <div>
                        <p class="section-kicker">"Primary flow"</p>
                        <h2>"Tutor context"</h2>
                    </div>
                    <button
                        class="btn-primary"
                        disabled=fetch_context.pending()
                        on:click=move |_| {
                            copied.set(false);
                            fetch_context.dispatch(());
                        }
                    >
                        "Fetch context"
                    </button>
                </div>
                <p class="panel-note-inline">"Paste this context into the tutor before the lesson."</p>

                {move || {
                    context_text
                        .get()
                        .map(|text| {
                            view! {
                                <div class="context-block">
                                    <div class="context-actions">
                                        <span class="entity-meta">"Structured learner state"</span>
                                        <button on:click=move |_| {
                                            copy.dispatch(());
                                        }>{move || if copied.get() { "Copied" } else { "Copy" }}</button>
                                    </div>
                                    <pre class="context">{text}</pre>
                                </div>
                            }
                        })
                }}
            </section>

            <section class="section-block">
                <div class="section-heading">
                    <div>
                        <p class="section-kicker">"Archive"</p>
                        <h2>"Data export"</h2>
                    </div>
                </div>
                {move || {
                    active
                        .id()
                        .map(|id| {
                            view! {
                                <div class="export-links">
                                    <a href=format!("/api/v1/export/full?language_profile_id={id}")>
                                        <span>"Full JSON export"</span>
                                        <span aria-hidden="true">"↗"</span>
                                    </a>
                                    <a href=format!("/api/v1/export/cards.csv?language_profile_id={id}")>
                                        <span>"Cards CSV"</span>
                                        <span aria-hidden="true">"↗"</span>
                                    </a>
                                </div>
                            }
                        })
                }}
            </section>
        </Page>
    }
}
