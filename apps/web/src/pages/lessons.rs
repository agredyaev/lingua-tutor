//! Lesson history (09-frontend.md §3 `/lessons`).

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::{ApiError, LessonDto};

/// `/lessons` (detail inline; lessons are read-mostly records).
#[component]
pub fn LessonsPage() -> impl IntoView {
    let active = use_active_profile();
    let lessons = LocalResource::new(move || {
        let id = active.id();
        async move {
            let Some(id) = id else {
                return Ok::<Vec<LessonDto>, ApiError>(Vec::new());
            };
            api_client::call::<_, Vec<LessonDto>>(
                "list_lessons",
                &serde_json::json!({ "language_profile_id": id }),
            )
            .await
        }
    });

    view! {
        <Page>
            <header class="page-header">
                <div>
                    <p class="eyebrow">"History"</p>
                    <h1>"History"</h1>
                </div>
            </header>

            <Suspense fallback=|| view! { <p class="loading">"Loading lessons…"</p> }>
                {move || {
                    lessons
                        .get()
                        .map(|result| match result {
                            Ok(list) if list.is_empty() => {
                                view! {
                                    <div class="empty-state">
                                        <p class="empty-state-title">"No lessons yet"</p>
                                        <p>"Completed lessons will appear here with tutor notes and the next goal."</p>
                                    </div>
                                }
                                    .into_any()
                            }
                            Ok(list) => {
                                view! {
                                    <ul class="entity-list lesson-list">
                                        {list
                                            .iter()
                                            .map(|l| {
                                                let status = l.status.clone();
                                                view! {
                                                    <li class="lesson-row">
                                                        <div class="lesson-row-header">
                                                            <div class="lesson-title-group">
                                                                <span class="entity-index">
                                                                    {format!("#{}", l.lesson_number)}
                                                                </span>
                                                                <div>
                                                                    <p class="lesson-scenario">{l.scenario.clone()}</p>
                                                                    <p class="entity-meta">"Lesson record"</p>
                                                                </div>
                                                            </div>
                                                            <span class="badge">{status}</span>
                                                        </div>

                                                        {l.tutor_summary
                                                            .clone()
                                                            .map(|summary| {
                                                                view! {
                                                                    <div class="lesson-summary">
                                                                        <span class="section-kicker">"Tutor note"</span>
                                                                        <p>{summary}</p>
                                                                    </div>
                                                                }
                                                            })}

                                                        {l.next_goal
                                                            .clone()
                                                            .map(|goal| {
                                                                view! {
                                                                    <div class="lesson-next-goal">
                                                                        <span class="section-kicker">"Next goal"</span>
                                                                        <p>{goal}</p>
                                                                    </div>
                                                                }
                                                            })}
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
