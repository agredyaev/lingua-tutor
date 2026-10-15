//! Language profiles page: list, select active, create (08-http-api.md §2).

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::{ApiError, LanguagePackDto, LanguageProfileDto};

/// `/languages`.
#[component]
pub fn LanguagesPage() -> impl IntoView {
    let active = use_active_profile();
    let refresh = RwSignal::new(0_u32);
    let profiles = LocalResource::new(move || {
        refresh.track();
        async move {
            api_client::call::<_, Vec<LanguageProfileDto>>(
                "list_language_profiles",
                &serde_json::json!({}),
            )
            .await
        }
    });
    let packs = LocalResource::new(move || async move {
        api_client::call::<_, Vec<LanguagePackDto>>("list_language_packs", &serde_json::json!({}))
            .await
    });
    let error = RwSignal::new(Option::<String>::None);

    let create = Action::new_local(move |pack: &LanguagePackDto| {
        let pack_id = pack.id.clone();
        let source = pack.default_source_language.clone();
        async move {
            error.set(None);
            let result: Result<serde_json::Value, ApiError> = api_client::call(
                "create_language_profile",
                &serde_json::json!({ "language_pack_id": pack_id, "source_language": source }),
            )
            .await;
            match result {
                Ok(_) => refresh.update(|r| *r += 1),
                Err(e) => error.set(Some(e.to_string())),
            }
        }
    });

    view! {
        <Page>
            <h1>"Languages"</h1>
            {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}
            <Suspense fallback=|| view! { <p>"Loading…"</p> }>
                {move || {
                    profiles
                        .get()
                        .map(|result| match result {
                            Ok(list) => {
                                view! {
                                    <ul class="profiles">
                                        {list
                                            .iter()
                                            .map(|p| {
                                                let profile = p.clone();
                                                let selected = active.id() == Some(p.id.clone());
                                                view! {
                                                    <li>
                                                        <button on:click=move |_| {
                                                            active.chosen.set(Some(profile.clone()));
                                                        }>
                                                            {format!(
                                                                "{}{} — lesson {} ({})",
                                                                if selected { "✓ " } else { "" },
                                                                profile.language_pack_id,
                                                                profile.current_lesson_number,
                                                                profile.status,
                                                            )}
                                                        </button>
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
            <h2>"Add"</h2>
            <Suspense fallback=|| ()>
                {move || {
                    packs
                        .get()
                        .map(|result| match result {
                            Ok(list) => {
                                view! {
                                    <ul>
                                        {list
                                            .iter()
                                            .map(|pack| {
                                                let pack = pack.clone();
                                                view! {
                                                    <li>
                                                        <button
                                                            disabled=create.pending()
                                                            on:click=move |_| {
                                                                create.dispatch(pack.clone());
                                                            }
                                                        >
                                                            {format!(
                                                                "Start {} ({})",
                                                                pack.display_name,
                                                                pack.default_source_language,
                                                            )}
                                                        </button>
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
