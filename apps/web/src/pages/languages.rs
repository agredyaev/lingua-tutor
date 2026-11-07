//! Language profiles page: list, select active, create (08-local-rpc.md §2).

use crate::api_client;
use crate::app::{Page, use_active_profile};
use leptos::prelude::*;
use shared_schema::{ApiError, LanguageProfileDto};

const LANGUAGE_SUGGESTIONS: &[(&str, &str)] = &[
    ("ab", "Abkhazian"),
    ("aa", "Afar"),
    ("af", "Afrikaans"),
    ("ak", "Akan"),
    ("sq", "Albanian"),
    ("am", "Amharic"),
    ("ar", "Arabic"),
    ("an", "Aragonese"),
    ("hy", "Armenian"),
    ("as", "Assamese"),
    ("av", "Avaric"),
    ("ae", "Avestan"),
    ("ay", "Aymara"),
    ("az", "Azerbaijani"),
    ("bm", "Bambara"),
    ("ba", "Bashkir"),
    ("eu", "Basque"),
    ("be", "Belarusian"),
    ("bn", "Bengali"),
    ("bi", "Bislama"),
    ("bs", "Bosnian"),
    ("br", "Breton"),
    ("bg", "Bulgarian"),
    ("my", "Burmese"),
    ("ca", "Catalan"),
    ("ch", "Chamorro"),
    ("ce", "Chechen"),
    ("ny", "Chichewa"),
    ("zh", "Chinese"),
    ("cu", "Church Slavic"),
    ("cv", "Chuvash"),
    ("kw", "Cornish"),
    ("co", "Corsican"),
    ("cr", "Cree"),
    ("hr", "Croatian"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("dv", "Divehi"),
    ("nl", "Dutch"),
    ("dz", "Dzongkha"),
    ("en", "English"),
    ("eo", "Esperanto"),
    ("et", "Estonian"),
    ("ee", "Ewe"),
    ("fo", "Faroese"),
    ("fj", "Fijian"),
    ("fi", "Finnish"),
    ("fr", "French"),
    ("ff", "Fulah"),
    ("gl", "Galician"),
    ("lg", "Ganda"),
    ("ka", "Georgian"),
    ("de", "German"),
    ("el", "Greek"),
    ("gn", "Guarani"),
    ("gu", "Gujarati"),
    ("ht", "Haitian Creole"),
    ("ha", "Hausa"),
    ("he", "Hebrew"),
    ("hz", "Herero"),
    ("hi", "Hindi"),
    ("ho", "Hiri Motu"),
    ("hu", "Hungarian"),
    ("is", "Icelandic"),
    ("io", "Ido"),
    ("ig", "Igbo"),
    ("id", "Indonesian"),
    ("ia", "Interlingua"),
    ("ie", "Interlingue"),
    ("iu", "Inuktitut"),
    ("ik", "Inupiaq"),
    ("ga", "Irish"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("jv", "Javanese"),
    ("kl", "Kalaallisut"),
    ("kn", "Kannada"),
    ("kr", "Kanuri"),
    ("ks", "Kashmiri"),
    ("kk", "Kazakh"),
    ("km", "Khmer"),
    ("ki", "Kikuyu"),
    ("rw", "Kinyarwanda"),
    ("kv", "Komi"),
    ("kg", "Kongo"),
    ("ko", "Korean"),
    ("kj", "Kuanyama"),
    ("ku", "Kurdish"),
    ("ky", "Kyrgyz"),
    ("lo", "Lao"),
    ("la", "Latin"),
    ("lv", "Latvian"),
    ("li", "Limburgan"),
    ("ln", "Lingala"),
    ("lt", "Lithuanian"),
    ("lu", "Luba-Katanga"),
    ("lb", "Luxembourgish"),
    ("mk", "Macedonian"),
    ("mg", "Malagasy"),
    ("ms", "Malay"),
    ("ml", "Malayalam"),
    ("mt", "Maltese"),
    ("gv", "Manx"),
    ("mi", "Maori"),
    ("mr", "Marathi"),
    ("mh", "Marshallese"),
    ("mn", "Mongolian"),
    ("na", "Nauru"),
    ("nv", "Navajo"),
    ("ng", "Ndonga"),
    ("ne", "Nepali"),
    ("nd", "North Ndebele"),
    ("se", "Northern Sami"),
    ("no", "Norwegian"),
    ("nb", "Norwegian Bokmål"),
    ("nn", "Norwegian Nynorsk"),
    ("oc", "Occitan"),
    ("or", "Odia"),
    ("oj", "Ojibwa"),
    ("om", "Oromo"),
    ("os", "Ossetian"),
    ("pi", "Pali"),
    ("ps", "Pashto"),
    ("fa", "Persian"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("pa", "Punjabi"),
    ("qu", "Quechua"),
    ("ro", "Romanian"),
    ("rm", "Romansh"),
    ("rn", "Rundi"),
    ("ru", "Russian"),
    ("sm", "Samoan"),
    ("sg", "Sango"),
    ("sa", "Sanskrit"),
    ("sc", "Sardinian"),
    ("gd", "Scottish Gaelic"),
    ("sr", "Serbian"),
    ("sh", "Serbo-Croatian"),
    ("sn", "Shona"),
    ("ii", "Sichuan Yi"),
    ("sd", "Sindhi"),
    ("si", "Sinhala"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("so", "Somali"),
    ("nr", "South Ndebele"),
    ("st", "Southern Sotho"),
    ("es", "Spanish"),
    ("su", "Sundanese"),
    ("sw", "Swahili (macrolanguage)"),
    ("ss", "Swati"),
    ("sv", "Swedish"),
    ("tl", "Tagalog"),
    ("ty", "Tahitian"),
    ("tg", "Tajik"),
    ("ta", "Tamil"),
    ("tt", "Tatar"),
    ("te", "Telugu"),
    ("th", "Thai"),
    ("bo", "Tibetan"),
    ("ti", "Tigrinya"),
    ("to", "Tonga (Tonga Islands)"),
    ("ts", "Tsonga"),
    ("tn", "Tswana"),
    ("tr", "Turkish"),
    ("tk", "Turkmen"),
    ("tw", "Twi"),
    ("ug", "Uighur"),
    ("uk", "Ukrainian"),
    ("ur", "Urdu"),
    ("uz", "Uzbek"),
    ("ve", "Venda"),
    ("vi", "Vietnamese"),
    ("vo", "Volapük"),
    ("wa", "Walloon"),
    ("cy", "Welsh"),
    ("fy", "Western Frisian"),
    ("wo", "Wolof"),
    ("xh", "Xhosa"),
    ("yi", "Yiddish"),
    ("yo", "Yoruba"),
    ("za", "Zhuang"),
    ("zu", "Zulu"),
];

const TARGET_LOCALE_VARIANTS: &[(&str, &str)] = &[
    ("en-US", "English — United States"),
    ("en-GB", "English — United Kingdom"),
    ("en-CA", "English — Canada"),
    ("en-AU", "English — Australia"),
    ("en-NZ", "English — New Zealand"),
    ("en-IE", "English — Ireland"),
    ("es-ES", "Spanish — Spain"),
    ("es-MX", "Spanish — Mexico"),
    ("es-AR", "Spanish — Argentina"),
    ("es-CO", "Spanish — Colombia"),
    ("es-CL", "Spanish — Chile"),
    ("es-PE", "Spanish — Peru"),
    ("pt-PT", "Portuguese — Portugal"),
    ("pt-BR", "Portuguese — Brazil"),
    ("fr-FR", "French — France"),
    ("fr-CA", "French — Canada"),
    ("fr-BE", "French — Belgium"),
    ("fr-CH", "French — Switzerland"),
    ("de-DE", "German — Germany"),
    ("de-AT", "German — Austria"),
    ("de-CH", "German — Switzerland"),
    ("it-IT", "Italian — Italy"),
    ("it-CH", "Italian — Switzerland"),
    ("nl-NL", "Dutch — Netherlands"),
    ("nl-BE", "Dutch — Belgium"),
    ("zh-CN", "Chinese — Mainland China"),
    ("zh-TW", "Chinese — Taiwan"),
    ("zh-HK", "Chinese — Hong Kong"),
    ("zh-SG", "Chinese — Singapore"),
    ("ar-SA", "Arabic — Saudi Arabia"),
    ("ar-EG", "Arabic — Egypt"),
    ("ar-MA", "Arabic — Morocco"),
    ("ar-AE", "Arabic — United Arab Emirates"),
    ("sr-RS", "Serbian — Serbia"),
    ("sr-Latn-RS", "Serbian Latin — Serbia"),
    ("ca-ES", "Catalan — Spain"),
    ("eu-ES", "Basque — Spain"),
    ("gl-ES", "Galician — Spain"),
    ("nb-NO", "Norwegian Bokmål — Norway"),
    ("nn-NO", "Norwegian Nynorsk — Norway"),
    ("sv-SE", "Swedish — Sweden"),
    ("da-DK", "Danish — Denmark"),
    ("fi-FI", "Finnish — Finland"),
    ("pl-PL", "Polish — Poland"),
    ("cs-CZ", "Czech — Czechia"),
    ("sk-SK", "Slovak — Slovakia"),
    ("hu-HU", "Hungarian — Hungary"),
    ("ro-RO", "Romanian — Romania"),
    ("bg-BG", "Bulgarian — Bulgaria"),
    ("uk-UA", "Ukrainian — Ukraine"),
    ("ru-RU", "Russian — Russia"),
    ("el-GR", "Greek — Greece"),
    ("tr-TR", "Turkish — Türkiye"),
    ("he-IL", "Hebrew — Israel"),
    ("hi-IN", "Hindi — India"),
    ("id-ID", "Indonesian — Indonesia"),
    ("vi-VN", "Vietnamese — Vietnam"),
    ("th-TH", "Thai — Thailand"),
    ("ja-JP", "Japanese — Japan"),
    ("ko-KR", "Korean — South Korea"),
    ("af-ZA", "Afrikaans — South Africa"),
    ("sw-KE", "Swahili — Kenya"),
];

fn suggestion_matches(query: &str, value: &str, label: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || value.to_lowercase().contains(&query)
        || label.to_lowercase().contains(&query)
}
/// `/languages`.
#[component]
pub(crate) fn LanguagesPage() -> impl IntoView {
    let active = use_active_profile();
    let refresh = RwSignal::new(0_u32);
    let target_locale = RwSignal::new(String::new());
    let source_language = RwSignal::new(String::new());
    let target_search_open = RwSignal::new(false);
    let source_search_open = RwSignal::new(false);
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
    let error = RwSignal::new(Option::<String>::None);

    let create = Action::new_local(move |input: &(String, String)| {
        let target_locale = input.0.clone();
        let source_language = input.1.clone();
        async move {
            error.set(None);
            let result: Result<serde_json::Value, ApiError> = api_client::call(
                "create_language_profile",
                &serde_json::json!({
                    "target_locale": target_locale,
                    "source_language": source_language
                }),
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
            <header class="page-header">
                <div>
                    <p class="eyebrow">"Profiles"</p>
                    <h1>"Languages"</h1>
                    <p class="page-description">
                        "Each profile keeps its own learning history while the engine stays language-neutral."
                    </p>
                </div>
            </header>

            {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}

            <section class="section-block">
                <div class="section-heading">
                    <div>
                        <p class="section-kicker">"Active profiles"</p>
                        <h2>"Choose a language"</h2>
                    </div>
                </div>
                <Suspense fallback=|| view! { <p class="loading">"Loading profiles…"</p> }>
                    {move || {
                        profiles
                            .get()
                            .map(|result| match result {
                                Ok(list) if list.is_empty() => {
                                    view! {
                                        <div class="empty-state compact">
                                            <p class="empty-state-title">"No profiles yet"</p>
                                            <p>"Create the first language profile below."</p>
                                        </div>
                                    }
                                        .into_any()
                                }
                                Ok(list) => {
                                    view! {
                                        <ul class="entity-list profiles">
                                            {list
                                                .iter()
                                                .map(|p| {
                                                    let profile = p.clone();
                                                    let selected = active.id() == Some(p.id.clone());
                                                    view! {
                                                        <li class=("profile-row", true) class=("selected", selected)>
                                                            <button
                                                                class="profile-button"
                                                                on:click=move |_| {
                                                                    active.chosen.set(Some(profile.clone()));
                                                                }
                                                            >
                                                                <div>
                                                                    <p class="profile-locale">{p.target_locale.clone()}</p>
                                                                    <p class="entity-meta">
                                                                        {format!(
                                                                            "{} explanations · lesson {} · {}",
                                                                            p.source_language,
                                                                            p.current_lesson_number,
                                                                            p.status,
                                                                        )}
                                                                    </p>
                                                                </div>
                                                                <span class="profile-state">
                                                                    {if selected { "Selected" } else { "Select" }}
                                                                </span>
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
            </section>

            <section class="panel form-panel">
                <div class="section-heading">
                    <div>
                        <p class="section-kicker">"New profile"</p>
                        <h2>"Add a language"</h2>
                    </div>
                </div>
                <div class="form-grid">
                    <div class="field">
                        <label class="field-label" for="target-locale">"Target locale"</label>
                        <div class="language-search">
                            <input
                                id="target-locale"
                                type="text"
                                autocomplete="off"
                                aria-autocomplete="list"
                                placeholder="Search language or BCP 47 locale"
                                prop:value=move || target_locale.get()
                                on:focus=move |_| target_search_open.set(true)
                                on:blur=move |_| target_search_open.set(false)
                                on:input=move |ev| {
                                    target_locale.set(event_target_value(&ev));
                                    target_search_open.set(true);
                                }
                            />
                            <Show when=move || target_search_open.get()>
                                <div class="language-search-menu" role="listbox">
                                    {move || {
                                        let query = target_locale.get();
                                        TARGET_LOCALE_VARIANTS
                                            .iter()
                                            .chain(LANGUAGE_SUGGESTIONS.iter())
                                            .filter(|(value, label)| {
                                                suggestion_matches(&query, value, label)
                                            })
                                            .take(12)
                                            .map(|(value, label)| {
                                                let value = *value;
                                                let label = *label;
                                                view! {
                                                    <button
                                                        type="button"
                                                        class="language-search-option"
                                                        on:pointerdown=move |ev| {
                                                            ev.prevent_default();
                                                            target_locale.set(value.to_owned());
                                                            target_search_open.set(false);
                                                        }
                                                    >
                                                        <span>{label}</span>
                                                        <code>{value}</code>
                                                    </button>
                                                }
                                            })
                                            .collect::<Vec<_>>()
                                    }}
                                </div>
                            </Show>
                        </div>
                        <span class="field-hint">"Search by language name or code; custom BCP 47 tags are also accepted."</span>
                    </div>
                    <div class="field">
                        <label class="field-label" for="source-language">"Explanation language"</label>
                        <div class="language-search">
                            <input
                                id="source-language"
                                type="text"
                                autocomplete="off"
                                aria-autocomplete="list"
                                placeholder="Search explanation language"
                                prop:value=move || source_language.get()
                                on:focus=move |_| source_search_open.set(true)
                                on:blur=move |_| source_search_open.set(false)
                                on:input=move |ev| {
                                    source_language.set(event_target_value(&ev));
                                    source_search_open.set(true);
                                }
                            />
                            <Show when=move || source_search_open.get()>
                                <div class="language-search-menu" role="listbox">
                                    {move || {
                                        let query = source_language.get();
                                        LANGUAGE_SUGGESTIONS
                                            .iter()
                                            .filter(|(value, label)| {
                                                suggestion_matches(&query, value, label)
                                            })
                                            .take(12)
                                            .map(|(value, label)| {
                                                let value = *value;
                                                let label = *label;
                                                view! {
                                                    <button
                                                        type="button"
                                                        class="language-search-option"
                                                        on:pointerdown=move |ev| {
                                                            ev.prevent_default();
                                                            source_language.set(value.to_owned());
                                                            source_search_open.set(false);
                                                        }
                                                    >
                                                        <span>{label}</span>
                                                        <code>{value}</code>
                                                    </button>
                                                }
                                            })
                                            .collect::<Vec<_>>()
                                    }}
                                </div>
                            </Show>
                        </div>
                        <span class="field-hint">"Used only for short explanations when needed."</span>
                    </div>
                </div>
                <div class="form-actions">
                    <button
                        class="btn-primary"
                        disabled=create.pending()
                        on:click=move |_| {
                            create.dispatch((target_locale.get(), source_language.get()));
                        }
                    >
                        "Create profile"
                    </button>
                </div>
            </section>
        </Page>
    }
}
