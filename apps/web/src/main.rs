//! Leptos CSR PWA entry point. Routes and pages land at WBS 6.1
//! (09-frontend.md §3); M0 ships the mountable shell only.

use leptos::prelude::*;

/// Root view: application shell. Single route until `leptos_router`
/// arrives with the page set (WBS 6.1).
#[component]
fn App() -> impl IntoView {
    view! {
        <main>
            <h1>"Lingua Tutor"</h1>
        </main>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
