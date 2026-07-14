//! App shell: router, navigation, active-profile context, per-page
//! error boundary (09-frontend.md §2–3).

use crate::pages;
use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
use shared_schema::LanguageProfileDto;

/// Active language profile shared app-wide via context (09 §2).
/// Explicit user choice wins; otherwise the server default — a derived
/// signal, never an effect writing state (09 §2 effects rule).
#[derive(Clone, Copy)]
pub struct ActiveProfile {
    /// User selection from /languages.
    pub chosen: RwSignal<Option<LanguageProfileDto>>,
    default_from_server: Signal<Option<LanguageProfileDto>>,
}

impl ActiveProfile {
    /// Effective profile: user choice, else server default/first.
    pub fn current(&self) -> Option<LanguageProfileDto> {
        self.chosen.get().or_else(|| self.default_from_server.get())
    }

    /// Effective profile ID.
    pub fn id(&self) -> Option<String> {
        self.current().map(|p| p.id)
    }
}

/// Root component: context + navigation + routes.
#[component]
pub fn App() -> impl IntoView {
    let profiles = LocalResource::new(move || async move {
        crate::api_client::get::<Vec<LanguageProfileDto>>("/language-profiles").await
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
    provide_context(ActiveProfile {
        chosen: RwSignal::new(None),
        default_from_server,
    });

    view! {
        <Router>
            <nav class="topnav">
                <NavLink href="/dashboard" label="Dashboard" />
                <NavLink href="/review" label="Review" />
                <NavLink href="/cards" label="Cards" />
                <NavLink href="/stats" label="Stats" />
                <NavLink href="/lessons" label="Lessons" />
                <details class="nav-more">
                    <summary>"More"</summary>
                    <div class="nav-more-panel">
                        <a href="/import">"Import"</a>
                        <a href="/export">"Export"</a>
                        <a href="/languages">"Languages"</a>
                    </div>
                </details>
            </nav>
            <main>
                <Routes fallback=|| view! { <p>"Not found."</p> }>
                    <Route path=path!("/") view=pages::dashboard::DashboardPage />
                    <Route path=path!("/dashboard") view=pages::dashboard::DashboardPage />
                    <Route path=path!("/languages") view=pages::languages::LanguagesPage />
                    <Route path=path!("/languages/:profile_id") view=pages::languages::LanguagesPage />
                    <Route path=path!("/learn") view=pages::misc::LearnPage />
                    <Route path=path!("/review") view=pages::review::ReviewPage />
                    <Route path=path!("/review/summary") view=pages::review::SummaryPage />
                    <Route path=path!("/stats") view=pages::review::StatsPage />
                    <Route path=path!("/cards") view=pages::cards::CardsPage />
                    <Route path=path!("/cards/redesign") view=pages::cards::CardsPage />
                    <Route path=path!("/cards/:id") view=pages::cards::CardDetailPage />
                    <Route path=path!("/lessons") view=pages::lessons::LessonsPage />
                    <Route path=path!("/lessons/:id") view=pages::lessons::LessonsPage />
                    <Route path=path!("/learning-items") view=pages::records::ItemsPage />
                    <Route path=path!("/learning-items/:id") view=pages::records::ItemsPage />
                    <Route path=path!("/errors") view=pages::records::ErrorsPage />
                    <Route path=path!("/errors/:id") view=pages::records::ErrorsPage />
                    <Route path=path!("/listening") view=pages::records::ItemsPage />
                    <Route path=path!("/import") view=pages::import_export::ImportPage />
                    <Route path=path!("/export") view=pages::import_export::ExportPage />
                    <Route path=path!("/settings") view=pages::misc::SettingsPage />
                    <Route path=path!("/system") view=pages::misc::SystemPage />
                </Routes>
            </main>
        </Router>
    }
}

/// Primary nav link: highlights itself via `aria-current="page"` from the
/// router's current path, so active state needs no extra signal.
#[component]
fn NavLink(href: &'static str, label: &'static str) -> impl IntoView {
    let location = use_location();
    let active = move || {
        let path = location.pathname.get();
        path == href || (href == "/dashboard" && path == "/")
    };
    view! {
        <a href=href aria-current=move || active().then_some("page")>
            {label}
        </a>
    }
}

/// Per-page error boundary: render error + retry hint, never a blank screen
/// (09 §2).
#[component]
pub fn Page(children: Children) -> impl IntoView {
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
        }>{children()}</ErrorBoundary>
    }
}

/// Active profile from context.
///
/// # Panics
///
/// Never in practice: `App` provides the context before any page renders.
pub fn use_active_profile() -> ActiveProfile {
    #[allow(clippy::expect_used)] // invariant: App() provides before routing
    use_context::<ActiveProfile>().expect("ActiveProfile context provided by App")
}
