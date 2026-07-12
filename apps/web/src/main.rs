//! Leptos CSR PWA entry point (09-frontend.md). Routes per §3; binding
//! practices per §2 (LocalResource+Suspense reads, Action writes,
//! ErrorBoundary per page, context for the active profile).

mod api_client;
mod app;
mod offline;
mod pages;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
