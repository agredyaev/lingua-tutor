//! First-run setup for the local Secure MCP Tunnel client.

use crate::{api_client, app::Page};
use leptos::{html, prelude::*};
use leptos_router::{NavigateOptions, hooks::use_navigate};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
struct TunnelStatus {
    status: String,
}

#[component]
pub(crate) fn TunnelSetupPage() -> impl IntoView {
    let status = LocalResource::new(|| async {
        api_client::call::<_, TunnelStatus>("tunnel_status", &serde_json::json!({})).await
    });
    let tunnel_id = NodeRef::<html::Input>::new();
    let api_key = NodeRef::<html::Input>::new();
    let pending = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();
    let submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        if pending.get_untracked() {
            return;
        }
        let (Some(tunnel_id), Some(api_key)) = (tunnel_id.get_untracked(), api_key.get_untracked())
        else {
            return;
        };
        let params = serde_json::json!({
            "tunnel_id": tunnel_id.value(),
            "api_key": api_key.value(),
        });
        api_key.set_value("");
        pending.set(true);
        error.set(None);
        let navigate = navigate.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match api_client::call::<_, serde_json::Value>("connect_tunnel", &params).await {
                Ok(_) => navigate(
                    "/dashboard",
                    NavigateOptions {
                        replace: true,
                        ..Default::default()
                    },
                ),
                Err(err) => {
                    pending.set(false);
                    error.set(Some(err.to_string()));
                }
            }
        });
    };

    view! {
        <Page>
            <header class="page-header">
                <h1>"Connect your OpenAI tunnel"</h1>
            </header>
            <section class="panel tunnel-setup">
                <ol class="tunnel-steps">
                    <li>
                        <strong>"Create a tunnel. "</strong>
                        "In "
                        <a href="https://platform.openai.com/settings/organization/tunnels" target="_blank" rel="noopener noreferrer">"OpenAI Platform → Tunnels"</a>
                        ", use your default Platform organization and create a tunnel. Use "
                        <strong>"Lingua Tutor"</strong>
                        " as the name and "
                        <code>"Local language tutor MCP"</code>
                        " as the description. Add the ChatGPT workspace where you’ll use Lingua Tutor. Keep standard HTTPS and copy the Tunnel ID."
                    </li>
                    <li>
                        <strong>"Create a restricted runtime key and connect. "</strong>
                        "In the same Platform organization, open "
                        <a href="https://platform.openai.com/settings/organization/api-keys" target="_blank" rel="noopener noreferrer">"OpenAI Platform → API keys"</a>
                        ", create a secret key with "
                        <strong>"Restricted"</strong>
                        " permissions. Enable only "
                        <strong>"Tunnels → Read and Use"</strong>
                        " and leave the other permissions off, including Manage. To limit the key to one tunnel, give its owner the "
                        <strong>"User"</strong>
                        " role on that tunnel and keep the owner free of broader organization-wide tunnel access. Enter both values below and connect."
                        <div class="tunnel-connect">
                            <Suspense fallback=|| view! { <p role="status">"Checking tunnel status…"</p> }>
                                {move || status.get().map(|result| match result {
                                    Ok(status) if status.status == "connected" => {
                                        view! { <p role="status">"The tunnel is connected."</p> }.into_any()
                                    }
                                    Ok(status) if status.status == "disabled" => {
                                        view! { <p>"Tunnel setup is disabled for this run."</p> }.into_any()
                                    }
                                    Ok(_) => view! {
                                        <form class="form-grid" method="post" autocomplete="off" on:submit=submit.clone()>
                                            <label class="field">
                                                <span class="field-label">"Tunnel ID"</span>
                                                <input node_ref=tunnel_id type="text" placeholder="tunnel_…" spellcheck="false" required />
                                            </label>
                                            <label class="field">
                                                <span class="field-label">"Runtime API key"</span>
                                                <input node_ref=api_key type="password" autocomplete="off" required />
                                            </label>
                                            {move || error.get().map(|message| view! { <p class="error" role="alert">{message}</p> })}
                                            <button class="btn btn-primary tunnel-submit" type="submit" disabled=move || pending.get()>
                                                {move || if pending.get() { "Connecting…" } else { "Connect tunnel" }}
                                            </button>
                                        </form>
                                    }.into_any(),
                                    Err(error) => view! { <p class="error" role="alert">{error.to_string()}</p> }.into_any(),
                                })}
                            </Suspense>
                        </div>
                    </li>
                    <li>
                        <strong>"Add Lingua Tutor to ChatGPT. "</strong>
                        "Keep Lingua Tutor running with the tunnel connected. In "
                        <a href="https://chatgpt.com/plugins" target="_blank" rel="noopener noreferrer">"ChatGPT Plugins"</a>
                        ", choose "
                        <strong>"+ → Add custom MCP server"</strong>
                        ". Set the name to "
                        <strong>"Lingua Tutor"</strong>
                        " and choose "
                        <strong>"Connection → Tunnel"</strong>
                        ". In the tunnel picker, select "
                        <strong>"the Lingua Tutor tunnel you created in step 1"</strong>
                        " or paste its Tunnel ID. Set authentication to "
                        <strong>"No authentication"</strong>
                        ", acknowledge the dialog, and select "
                        <strong>"Create as a plugin"</strong>
                        ". Install the plugin, then type "
                        <code>"@"</code>
                        " in a chat and select Lingua Tutor. Try "
                        <q>"Use Lingua Tutor to list my language profiles with get_active_language_profile."</q>
                    </li>
                </ol>
                <p><a href="/dashboard">"Continue locally"</a></p>
            </section>
        </Page>
    }
}
