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
            api_client::get::<Vec<LessonDto>>(&format!("/lessons?language_profile_id={id}")).await
        }
    });
    view! {
        <Page>
            <h1>"Lessons"</h1>
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    lessons
                        .get()
                        .map(|result| match result {
                            Ok(list) => {
                                view! {
                                    <ul class="lessons">
                                        {list
                                            .iter()
                                            .map(|l| {
                                                view! {
                                                    <li>
                                                        <strong>
                                                            {format!("#{} {} ({})", l.lesson_number, l.scenario, l.status)}
                                                        </strong>
                                                        {l.tutor_summary
                                                            .clone()
                                                            .map(|s| view! { <p>{s}</p> })}
                                                        {l.next_goal
                                                            .clone()
                                                            .map(|g| view! { <p>"Next: " {g}</p> })}
                                                    </li>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </ul>
                                }
                                    .into_any()
                            }
                            Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_any(),
                        })
                }}
            </Suspense>
        </Page>
    }
}
