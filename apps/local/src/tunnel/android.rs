//! Outbound tunnel transport for the Android app.

use super::{
    HttpResponse, MAX_POLL_BODY_BYTES, TunnelRequest, request_headers, retry_after_ms, unix_seconds,
};
use axum::{
    body::{Body, to_bytes},
    http::{HeaderValue, Request},
};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use std::time::{Duration, Instant};

pub(super) async fn request(input: TunnelRequest) -> Result<HttpResponse, String> {
    let timeout = input.timeout.max(Duration::from_secs(1));
    tokio::time::timeout(timeout, send(input))
        .await
        .map_err(|_| "tunnel HTTP request timed out".to_owned())?
}

async fn send(input: TunnelRequest) -> Result<HttpResponse, String> {
    let mut request = Request::builder().method(input.method).uri(input.url);
    for (name, value) in request_headers(
        &input.api_key,
        &input.instance_id,
        input.shard_token.as_deref(),
        input.body.is_some(),
    ) {
        let mut value =
            HeaderValue::from_str(&value).map_err(|_| "invalid tunnel HTTP header".to_owned())?;
        value.set_sensitive(name == "Authorization");
        request = request.header(name, value);
    }
    let request = request
        .body(Body::from(input.body.unwrap_or_default()))
        .map_err(|_| "invalid tunnel HTTP request".to_owned())?;
    let mut tcp = HttpConnector::new();
    tcp.enforce_http(false);
    tcp.set_connect_timeout(Some(Duration::from_secs(10)));
    let https = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .wrap_connector(tcp);
    let client = Client::builder(TokioExecutor::new()).build(https);
    let response = client
        .request(request)
        .await
        .map_err(|error| error.to_string())?;
    let received_at = Instant::now();
    let (parts, body) = response.into_parts();
    let retry_after_ms = parts
        .headers
        .get("retry-after")
        .and_then(|header| header.to_str().ok())
        .and_then(|value| retry_after_ms(value, unix_seconds()));
    let body = to_bytes(Body::new(body), MAX_POLL_BODY_BYTES)
        .await
        .map_err(|error| format!("could not read tunnel response: {error}"))?;
    Ok(HttpResponse {
        status: parts.status.as_u16(),
        body: body.to_vec(),
        received_at,
        retry_after_ms,
    })
}
