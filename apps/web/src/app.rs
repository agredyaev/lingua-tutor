//! App shell: router, navigation, active-profile context, per-page
//! error boundary (09-frontend.md §2–3).

use crate::pages;
use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
use shared_schema::{ApiError, LanguageProfileDto};

/// Active language profile shared app-wide via context (09 §2).
/// Explicit user choice wins; otherwise the server default — a derived
/// signal, never an effect writing state (09 §2 effects rule).
#[derive(Clone, Copy)]
pub(crate) struct ActiveProfile {
    /// User selection from /languages.
    pub chosen: RwSignal<Option<LanguageProfileDto>>,
    default_from_server: Signal<Option<LanguageProfileDto>>,
    default_lookup_network_failed: Signal<bool>,
}

impl ActiveProfile {
    /// Effective profile: user choice, else server default/first.
    pub(crate) fn current(&self) -> Option<LanguageProfileDto> {
        self.chosen.get().or_else(|| self.default_from_server.get())
    }

    /// Effective profile ID.
    pub(crate) fn id(&self) -> Option<String> {
        self.current().map(|p| p.id)
    }

    /// Whether resolving the server-default profile failed because the local
    /// runtime is unreachable. Review uses this to distinguish an offline
    /// reload from "no profile selected" without issuing a duplicate RPC.
    pub(crate) fn default_lookup_network_failed(&self) -> bool {
        self.default_lookup_network_failed.get()
    }
}

/// Root component: context + navigation + routes.
#[component]
pub(crate) fn App() -> impl IntoView {
    let profiles = LocalResource::new(move || async move {
        crate::api_client::call::<_, Vec<LanguageProfileDto>>(
            "list_language_profiles",
            &serde_json::json!({}),
        )
        .await
    });
    let default_from_server = Signal::derive(move || {
        profiles.get().and_then(|result| {
            result.ok().and_then(|list| {
                list.iter()
                    .find(|p| p.is_default)
                    .or_else(|| list.first())
                    .cloned()
            })
        })
    });
    let default_lookup_network_failed =
        Signal::derive(move || matches!(profiles.get(), Some(Err(ApiError::Network(_)))));
    provide_context(ActiveProfile {
        chosen: RwSignal::new(None),
        default_from_server,
        default_lookup_network_failed,
    });
    let sidebar_collapsed = RwSignal::new(false);

    view! {
        <Router>
            <nav
                class="topnav"
                class=("sidebar-collapsed", move || sidebar_collapsed.get())
                aria-label="Primary"
            >
                <div class="sidebar-head">
                    <a class="brand" href="/dashboard" aria-label="Lingua Tutor home">
                        <span class="brand-mark">"L"</span>
                        <span class="brand-name nav-label">"Lingua"</span>
                    </a>
                    <button
                        class="sidebar-toggle"
                        type="button"
                        aria-label=move || {
                            if sidebar_collapsed.get() {
                                "Expand sidebar"
                            } else {
                                "Collapse sidebar"
                            }
                        }
                        title=move || {
                            if sidebar_collapsed.get() {
                                "Expand sidebar"
                            } else {
                                "Collapse sidebar"
                            }
                        }
                        on:click=move |_| sidebar_collapsed.update(|collapsed| *collapsed = !*collapsed)
                    >
                        <svg viewBox="0 0 16 16" aria-hidden="true">
                            <path d="M9.5 4.5 6 8l3.5 3.5"></path>
                        </svg>
                    </button>
                </div>
                <NavLink href="/dashboard" label="Home" icon="dashboard" />
                <NavLink href="/review" label="Review" icon="review" />
                <NavLink href="/cards" label="Library" icon="cards" />
                <NavLink href="/stats" label="Progress" icon="stats" />
                <NavLink href="/lessons" label="History" icon="lessons" />
                <div class="nav-spacer"></div>
                <NavLink href="/languages" label="Languages" icon="languages" />
            </nav>
            <main class=("sidebar-collapsed-main", move || sidebar_collapsed.get())>
                <Routes fallback=|| view! { <p>"Not found."</p> }>
                    <Route path=path!("/") view=pages::dashboard::DashboardPage />
                    <Route path=path!("/dashboard") view=pages::dashboard::DashboardPage />
                    <Route path=path!("/languages") view=pages::languages::LanguagesPage />
                    <Route path=path!("/learn") view=pages::misc::LearnPage />
                    <Route path=path!("/review") view=pages::review::ReviewPage />
                    <Route path=path!("/review/summary") view=pages::review::SummaryPage />
                    <Route path=path!("/stats") view=pages::review::StatsPage />
                    <Route path=path!("/cards") view=pages::cards::CardsPage />
                    <Route path=path!("/cards/:id") view=pages::cards::CardDetailPage />
                    <Route path=path!("/lessons") view=pages::lessons::LessonsPage />
                    <Route path=path!("/learning-items") view=pages::records::ItemsPage />
                    <Route path=path!("/errors") view=pages::records::ErrorsPage />
                    <Route path=path!("/import") view=pages::import_export::ImportPage />
                    <Route path=path!("/export") view=pages::import_export::ExportPage />
                    <Route path=path!("/system") view=pages::misc::SystemPage />
                </Routes>
            </main>
        </Router>
    }
}

/// Primary nav link: highlights itself via `aria-current="page"` from the
/// router's current path, so active state needs no extra signal.
#[component]
fn NavLink(href: &'static str, label: &'static str, icon: &'static str) -> impl IntoView {
    let location = use_location();
    let active = move || {
        let path = location.pathname.get();
        path == href || (href == "/dashboard" && path == "/")
    };
    view! {
        <a
            href=href
            title=label
            aria-label=label
            aria-current=move || active().then_some("page")
        >
            <NavIcon kind=icon />
            <span class="nav-label">{label}</span>
        </a>
    }
}

#[component]
fn NavIcon(kind: &'static str) -> impl IntoView {
    match kind {
        "dashboard" => view! {
            <svg class="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M2.75 2.75h4.5v4.5h-4.5zM8.75 2.75h4.5v4.5h-4.5zM2.75 8.75h4.5v4.5h-4.5zM8.75 8.75h4.5v4.5h-4.5z"></path>
            </svg>
        }
            .into_any(),
        "review" => view! {
            <svg class="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M8 2.5a5.5 5.5 0 1 1-4.1 1.84M2.75 2.75v3.5h3.5"></path>
            </svg>
        }
            .into_any(),
        "cards" => view! {
            <svg class="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M3 4.25h8.75v8.5H3zM5.25 2.75H13v7.5"></path>
            </svg>
        }
            .into_any(),
        "stats" => view! {
            <svg class="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M3 12.75V9.5M8 12.75v-6.5M13 12.75v-10"></path>
            </svg>
        }
            .into_any(),
        "lessons" => view! {
            <svg class="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M3 2.75h8.5a1.5 1.5 0 0 1 1.5 1.5v9H4.5A1.5 1.5 0 0 1 3 11.75zM3 11.75a1.5 1.5 0 0 1 1.5-1.5H13"></path>
            </svg>
        }
            .into_any(),
        "languages" => view! {
            <svg class="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
                <circle cx="8" cy="8" r="5.25"></circle>
                <path d="M2.9 8h10.2M8 2.75c1.35 1.45 2 3.2 2 5.25s-.65 3.8-2 5.25M8 2.75C6.65 4.2 6 5.95 6 8s.65 3.8 2 5.25"></path>
            </svg>
        }
            .into_any(),
        _ => view! {
            <svg class="nav-icon" viewBox="0 0 16 16" aria-hidden="true">
                <circle cx="3.5" cy="8" r="0.75"></circle>
                <circle cx="8" cy="8" r="0.75"></circle>
                <circle cx="12.5" cy="8" r="0.75"></circle>
            </svg>
        }
            .into_any(),
    }
}

/// Per-page error boundary: render error + retry hint, never a blank screen
/// (09 §2).
#[component]
pub(crate) fn Page(children: Children) -> impl IntoView {
    view! {
        <ErrorBoundary fallback=|errors| {
            view! {
                <div class="error" role="alert">
                    <p>"Something failed:"</p>
                    <ul>
                        {move || {
                            errors
                                .get()
                                .into_iter()
                                .map(|(_, e)| view! { <li>{e.to_string()}</li> })
                                .collect::<Vec<_>>()
                        }}
                    </ul>
                    <p>"Reload the page to retry."</p>
                </div>
            }
        }>
            <div class="page">{children()}</div>
        </ErrorBoundary>
    }
}

/// Active profile from context.
///
/// # Panics
///
/// Never in practice: `App` provides the context before any page renders.
pub(crate) fn use_active_profile() -> ActiveProfile {
    #[allow(clippy::expect_used)] // invariant: App() provides before routing
    use_context::<ActiveProfile>().expect("ActiveProfile context provided by App")
}
