//! Native OpenAI Secure MCP Tunnel protocol adapted from Rivet.
//!
//! Wire parsing/backoff/terminal-response semantics come from Rivet. HTTPS is
//! performed with the macOS system curl binary. Sensitive headers and response
//! payloads are provided through curl's stdin config, never argv or disk.

use crate::{
    Env,
    mcp::{self, DirectDispatch},
    tunnel_policy::{PostClass, backoff_base_ms, classify_post},
    tunnel_wire::{
        CommandType, WireCommand, extract_jsonrpc_id, parse_command, parse_poll,
        parse_response_timeout, valid_tunnel_id,
    },
};
use serde::Serialize;
use serde_json::{Value, value::RawValue};
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{task::JoinSet, time::sleep};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
const SERVER_INFO: &str =
    r#"{"version":2,"channels":[{"name":"main","stateless":true,"proc_affinity":true}]}"#;
const WIRE_VERSION: &str = "2026-08-25";
const POLL_TIMEOUT_MS: u64 = 15_000;
const MAX_POLL_BODY_BYTES: usize = 4 * 1024 * 1024;
const MAX_IN_FLIGHT: usize = 16;
const CURL_PATH: &str = "/usr/bin/curl";

/// Native tunnel connection settings.
///
/// The runtime API key is only held in process memory. The CLI never persists
/// tunnel credentials.
#[derive(Debug, Clone)]
pub struct TunnelConfig {
    /// OpenAI tunnel ID.
    pub tunnel_id: String,
    /// Runtime API key.
    api_key: String,
    /// Control-plane base URL.
    base_url: String,
}

impl TunnelConfig {
    /// Build production OpenAI tunnel settings.
    pub fn production(tunnel_id: String, api_key: String) -> Result<Self, String> {
        Self::new(tunnel_id, api_key, DEFAULT_BASE_URL.to_owned())
    }

    fn new(tunnel_id: String, api_key: String, base_url: String) -> Result<Self, String> {
        if !valid_tunnel_id(&tunnel_id) {
            return Err("invalid tunnel ID format".to_owned());
        }
        if api_key.trim().is_empty() {
            return Err("runtime API key cannot be empty".to_owned());
        }
        Ok(Self {
            tunnel_id,
            api_key,
            base_url,
        })
    }
}

#[derive(Serialize)]
struct ResponseHeaders {
    #[serde(rename = "Content-Type")]
    content_type: [&'static str; 1],
}

#[derive(Serialize)]
struct TunnelResponse<'a> {
    request_id: &'a str,
    channel: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    resp_json: Option<&'a RawValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resp_headers: Option<ResponseHeaders>,
    resp_code: u16,
    resp_type: &'static str,
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

struct Client {
    config: TunnelConfig,
    endpoint: String,
}

impl Client {
    fn new(config: TunnelConfig) -> Result<Arc<Self>, String> {
        if !std::path::Path::new(CURL_PATH).is_file() {
            return Err(format!(
                "required macOS system curl is missing: {CURL_PATH}"
            ));
        }
        let endpoint = format!(
            "{}/v1/tunnels/{}",
            config.base_url.trim_end_matches('/'),
            config.tunnel_id
        );
        Ok(Arc::new(Self { config, endpoint }))
    }

    async fn validate(&self) -> Result<(), String> {
        let response = self
            .request(
                "GET",
                self.endpoint.clone(),
                None,
                None,
                Duration::from_secs(10),
            )
            .await?;
        match response.status {
            200 => Ok(()),
            401 | 403 => Err("tunnel authentication or authorization failed".to_owned()),
            status => Err(format!("tunnel metadata request returned HTTP {status}")),
        }
    }

    async fn poll_loop(
        self: &Arc<Self>,
        env: Env,
        registry: Arc<language_pack_registry::LanguagePackRegistry>,
    ) -> Result<(), String> {
        let mut in_flight = JoinSet::new();
        let mut failures = 0_u32;
        let poll_url = format!("{}/poll", self.endpoint);

        loop {
            while in_flight.len() >= MAX_IN_FLIGHT {
                if let Some(result) = in_flight.join_next().await {
                    report_task(result);
                }
            }
            while let Some(result) = in_flight.try_join_next() {
                report_task(result);
            }

            let available = MAX_IN_FLIGHT.saturating_sub(in_flight.len()).clamp(1, 25);
            let url = format!("{poll_url}?limit={available}&timeout_ms={POLL_TIMEOUT_MS}");
            let response = match self
                .request(
                    "GET",
                    url,
                    None,
                    None,
                    Duration::from_millis(POLL_TIMEOUT_MS + 5_000),
                )
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    failures = failures.saturating_add(1);
                    eprintln!("Tunnel poll transport error: {error}");
                    sleep(backoff(failures)).await;
                    continue;
                }
            };
            let received_at = Instant::now();

            match response.status {
                204 => {
                    failures = 0;
                    continue;
                }
                401 | 403 => {
                    return Err(format!(
                        "tunnel poll authorization failed with HTTP {}",
                        response.status
                    ));
                }
                429 | 500..=599 => {
                    failures = failures.saturating_add(1);
                    sleep(backoff(failures)).await;
                    continue;
                }
                200 => {}
                status => return Err(format!("tunnel poll returned HTTP {status}")),
            }

            if response.body.len() > MAX_POLL_BODY_BYTES {
                return Err(format!(
                    "tunnel response exceeds {MAX_POLL_BODY_BYTES} bytes"
                ));
            }
            failures = 0;
            let envelope = parse_poll(&response.body)
                .map_err(|error| format!("invalid tunnel poll JSON: {error}"))?;

            for raw in envelope.commands {
                let client = Arc::clone(self);
                let command_env = env.clone();
                let command_registry = Arc::clone(&registry);
                in_flight.spawn(async move {
                    process_command(client, command_env, command_registry, raw, received_at).await
                });
            }
        }
    }

    async fn post_terminal(
        &self,
        command: &WireCommand,
        body: Vec<u8>,
        deadline: Option<Instant>,
    ) -> Result<(), String> {
        let response_url = format!("{}/response", self.endpoint);
        let mut attempt = 0_u32;

        loop {
            if expired(deadline) {
                return Ok(());
            }
            attempt = attempt.saturating_add(1);
            let remaining = deadline
                .map(|value| value.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_secs(20))
                .min(Duration::from_secs(20));
            if remaining.is_zero() {
                return Ok(());
            }

            match self
                .request(
                    "POST",
                    response_url.clone(),
                    Some(body.clone()),
                    Some(command.shard_token.clone()),
                    remaining,
                )
                .await
            {
                Ok(response) => match classify_post(response.status) {
                    PostClass::Accepted | PostClass::Terminal => return Ok(()),
                    PostClass::Auth => {
                        return Err(format!(
                            "terminal response authorization failed with HTTP {}",
                            response.status
                        ));
                    }
                    PostClass::Fail => {
                        return Err(format!(
                            "terminal response rejected with HTTP {}",
                            response.status
                        ));
                    }
                    PostClass::Retry => {
                        if !sleep_with_deadline(backoff(attempt), deadline).await {
                            return Ok(());
                        }
                    }
                },
                Err(error) => {
                    eprintln!("Terminal response transport error: {error}");
                    if !sleep_with_deadline(backoff(attempt), deadline).await {
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn request(
        &self,
        method: &'static str,
        url: String,
        body: Option<Vec<u8>>,
        shard_token: Option<String>,
        timeout: Duration,
    ) -> Result<HttpResponse, String> {
        let api_key = self.config.api_key.clone();
        tokio::task::spawn_blocking(move || {
            curl_request(
                method,
                &url,
                &api_key,
                shard_token.as_deref(),
                body.as_deref(),
                timeout,
            )
        })
        .await
        .map_err(|error| format!("curl task failed: {error}"))?
    }
}

async fn process_command(
    client: Arc<Client>,
    env: Env,
    registry: Arc<language_pack_registry::LanguagePackRegistry>,
    raw: Box<RawValue>,
    received_at: Instant,
) -> Result<(), String> {
    let command = match parse_command(raw.as_ref()) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("Dropping malformed tunnel command: {error}");
            return Ok(());
        }
    };
    if command.request_id.is_empty() || command.shard_token.is_empty() {
        return Ok(());
    }

    let deadline = command
        .response_timeout
        .as_ref()
        .and_then(parse_response_timeout)
        .and_then(|duration| received_at.checked_add(duration));
    if expired(deadline) {
        return Ok(());
    }

    let terminal = match command.command_type {
        CommandType::SessionTermination => {
            encode_response(&command, None, 204, "session_termination_response")?
        }
        CommandType::Jsonrpc if command.channel.as_deref().unwrap_or("main") != "main" => {
            synthetic_error(&command, "client_internal", 502)?
        }
        CommandType::Jsonrpc => match command.jsonrpc.as_deref() {
            Some(jsonrpc) => dispatch_jsonrpc(&env, &registry, &command, jsonrpc)?,
            None => synthetic_error(&command, "protocol", 502)?,
        },
        CommandType::Other => synthetic_error(&command, "protocol", 502)?,
    };

    if terminal.is_empty() || expired(deadline) {
        return Ok(());
    }
    client.post_terminal(&command, terminal, deadline).await
}

fn dispatch_jsonrpc(
    env: &Env,
    registry: &language_pack_registry::LanguagePackRegistry,
    command: &WireCommand,
    jsonrpc: &RawValue,
) -> Result<Vec<u8>, String> {
    match mcp::dispatch_jsonrpc(env, registry, jsonrpc.get().as_bytes()) {
        Ok(DirectDispatch::Response(response)) => {
            let raw = RawValue::from_string(response).map_err(|error| error.to_string())?;
            encode_response(command, Some(raw.as_ref()), 200, "jsonrpc_response")
        }
        Ok(DirectDispatch::NotificationAck) => encode_response(command, None, 204, "notify_ack"),
        Err(_) => synthetic_error(command, "client_internal", 502),
    }
}

fn synthetic_error(
    command: &WireCommand,
    source: &'static str,
    status: u16,
) -> Result<Vec<u8>, String> {
    let id = command
        .jsonrpc
        .as_deref()
        .and_then(extract_jsonrpc_id)
        .unwrap_or(Value::Null);
    let raw = serde_json::value::to_raw_value(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32603,
            "message": "Bad Gateway",
            "data": {
                "tunnel_failure": {
                    "version": 1,
                    "source": source,
                    "upstream_response_received": false
                }
            }
        }
    }))
    .map_err(|error| error.to_string())?;
    encode_response(command, Some(raw.as_ref()), status, "jsonrpc_response")
}

fn encode_response(
    command: &WireCommand,
    jsonrpc: Option<&RawValue>,
    status: u16,
    response_type: &'static str,
) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&TunnelResponse {
        request_id: &command.request_id,
        channel: command.channel.as_deref().unwrap_or("main"),
        resp_json: jsonrpc,
        resp_headers: jsonrpc.map(|_| ResponseHeaders {
            content_type: ["application/json"],
        }),
        resp_code: status,
        resp_type: response_type,
    })
    .map_err(|error| error.to_string())
}

fn curl_request(
    method: &str,
    url: &str,
    api_key: &str,
    shard_token: Option<&str>,
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<HttpResponse, String> {
    let marker = "__LINGUA_TUNNEL_HTTP_STATUS__:";
    let timeout_secs = timeout.as_secs_f64().max(1.0);
    let mut config = String::new();
    config.push_str("silent\nshow-error\n");
    config.push_str(&format!("request = {}\n", curl_quote(method)));
    config.push_str(&format!("url = {}\n", curl_quote(url)));
    config.push_str(&format!("max-time = {:.3}\n", timeout_secs));
    config.push_str("connect-timeout = 10\n");
    config.push_str(&format!(
        "header = {}\n",
        curl_quote(&format!("Authorization: Bearer {api_key}"))
    ));
    config.push_str(&format!(
        "header = {}\n",
        curl_quote("X-Tunnel-Client-Name: lingua-tutor")
    ));
    config.push_str(&format!(
        "header = {}\n",
        curl_quote(&format!(
            "X-Tunnel-Client-Version: {}",
            env!("CARGO_PKG_VERSION")
        ))
    ));
    config.push_str(&format!(
        "header = {}\n",
        curl_quote(&format!(
            "X-Tunnel-Client-Wire-Protocol-Version: {}",
            WIRE_VERSION
        ))
    ));
    config.push_str(&format!(
        "header = {}\n",
        curl_quote(&format!("X-Tunnel-MCP-Server-Info: {SERVER_INFO}"))
    ));
    config.push_str(&format!(
        "header = {}\n",
        curl_quote(&format!("X-Tunnel-Client-Instance-Id: {}", instance_id()))
    ));
    if let Some(shard_token) = shard_token {
        config.push_str(&format!(
            "header = {}\n",
            curl_quote(&format!("X-Tunnel-Shard-Token: {shard_token}"))
        ));
    }
    if let Some(body) = body {
        let text = std::str::from_utf8(body)
            .map_err(|_| "tunnel terminal response is not UTF-8".to_owned())?;
        config.push_str(&format!(
            "header = {}\n",
            curl_quote("Content-Type: application/json")
        ));
        config.push_str(&format!("data-binary = {}\n", curl_quote(text)));
    }
    config.push_str(&format!(
        "write-out = {}\n",
        curl_quote(&format!("\n{marker}%{{http_code}}"))
    ));

    let mut child = Command::new(CURL_PATH)
        .arg("--config")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start {CURL_PATH}: {error}"))?;
    child
        .stdin
        .take()
        .ok_or("failed to open curl stdin")?
        .write_all(config.as_bytes())
        .map_err(|error| format!("failed to write curl config: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for curl: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl transport failed: {}", stderr.trim()));
    }

    let marker = marker.as_bytes();
    let Some(position) = find_last(&output.stdout, marker) else {
        return Err("curl response did not contain HTTP status marker".to_owned());
    };
    let status_text = std::str::from_utf8(&output.stdout[position + marker.len()..])
        .map_err(|_| "invalid curl HTTP status".to_owned())?
        .trim();
    let status = status_text
        .parse::<u16>()
        .map_err(|_| format!("invalid curl HTTP status: {status_text}"))?;
    let mut body = output.stdout[..position].to_vec();
    if body.last() == Some(&b'\n') {
        body.pop();
    }

    Ok(HttpResponse { status, body })
}

fn curl_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(backoff_base_ms(attempt))
}

fn expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

async fn sleep_with_deadline(delay: Duration, deadline: Option<Instant>) -> bool {
    if let Some(deadline) = deadline {
        let now = Instant::now();
        if now >= deadline || now.checked_add(delay).is_none_or(|wake| wake >= deadline) {
            return false;
        }
    }
    sleep(delay).await;
    true
}

fn report_task(result: Result<Result<(), String>, tokio::task::JoinError>) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("Tunnel command failed: {error}"),
        Err(error) => eprintln!("Tunnel command task failed: {error}"),
    }
}

fn instance_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("lingua-tutor-{}-{nanos}", std::process::id())
}

/// Connect Lingua Tutor's in-process MCP service to OpenAI Secure MCP Tunnel.
pub async fn run(
    env: Env,
    registry: Arc<language_pack_registry::LanguagePackRegistry>,
    config: TunnelConfig,
) -> Result<(), String> {
    let client = Client::new(config)?;
    println!("  tunnel: connecting");
    client.validate().await?;
    println!("  tunnel: connected");

    client.poll_loop(env, registry).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_quote_keeps_secrets_out_of_argv_shape() {
        let quoted = curl_quote("a\"b\\c\n");
        assert_eq!(quoted, "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn tunnel_config_rejects_bad_ids() {
        assert!(TunnelConfig::production("bad".to_owned(), "key".to_owned()).is_err());
    }
}
