//! One browser transport: JSON-RPC 2.0 over POST /rpc.
//! Components name application operations instead of encoding them as URL paths.

use serde::Serialize;
use serde::de::DeserializeOwned;
use shared_schema::{ApiError, ApiErrorBody};

pub async fn call<P: Serialize, T: DeserializeOwned>(
    method: &str,
    params: &P,
) -> Result<T, ApiError> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let request = gloo_net::http::Request::post("/rpc")
        .json(&payload)
        .map_err(|error| ApiError::Network(error.to_string()))?;
    let response = request
        .send()
        .await
        .map_err(|error| ApiError::Network(error.to_string()))?;
    let text = response
        .text()
        .await
        .map_err(|error| ApiError::Network(error.to_string()))?;
    let envelope: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| ApiError::Decode(error.to_string()))?;

    if let Some(error) = envelope.get("error") {
        let status = error
            .pointer("/data/status")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .unwrap_or(500);
        let body = error.pointer("/data/body");
        let detail =
            body.and_then(|value| serde_json::from_value::<ApiErrorBody>(value.clone()).ok());
        let code = detail
            .as_ref()
            .map(|body| body.error.code.clone())
            .unwrap_or_else(|| {
                error
                    .get("code")
                    .map(serde_json::Value::to_string)
                    .unwrap_or_else(|| "rpc_error".to_owned())
            });
        let message = detail
            .map(|body| body.error.message)
            .or_else(|| {
                error
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "RPC operation failed".to_owned());
        return Err(ApiError::Api {
            status,
            code,
            message,
        });
    }

    let result = envelope
        .get("result")
        .cloned()
        .ok_or_else(|| ApiError::Decode("JSON-RPC response missing result".to_owned()))?;
    serde_json::from_value(result).map_err(|error| ApiError::Decode(error.to_string()))
}
