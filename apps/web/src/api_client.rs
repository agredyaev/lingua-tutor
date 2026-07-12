//! One typed client over `shared-schema` DTOs (09-frontend.md §1).
//! No handwritten JSON in components; every failure is a typed [`ApiError`].

use serde::Serialize;
use serde::de::DeserializeOwned;
use shared_schema::{ApiError, ApiErrorBody};

const BASE: &str = "/api/v1";

async fn decode<T: DeserializeOwned>(response: gloo_net::http::Response) -> Result<T, ApiError> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?;
    if (200..300).contains(&status) {
        serde_json::from_str(&text).map_err(|e| ApiError::Decode(e.to_string()))
    } else {
        let (code, message) = serde_json::from_str::<ApiErrorBody>(&text).map_or_else(
            |_| ("http_error".to_owned(), text.clone()),
            |body| (body.error.code, body.error.message),
        );
        Err(ApiError::Api {
            status,
            code,
            message,
        })
    }
}

/// GET returning a DTO.
///
/// # Errors
///
/// [`ApiError`] on network failure, API error body, or decode mismatch.
pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let response = gloo_net::http::Request::get(&format!("{BASE}{path}"))
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?;
    decode(response).await
}

/// GET returning the raw body text (tutor-context copy flow).
///
/// # Errors
///
/// [`ApiError`] on network failure or non-2xx status.
pub async fn get_text(path: &str) -> Result<String, ApiError> {
    let response = gloo_net::http::Request::get(&format!("{BASE}{path}"))
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?;
    if (200..300).contains(&status) {
        Ok(text)
    } else {
        Err(ApiError::Api {
            status,
            code: "http_error".to_owned(),
            message: text,
        })
    }
}

/// POST with a JSON body, returning a DTO.
///
/// # Errors
///
/// [`ApiError`] on network failure, API error body, or decode mismatch.
pub async fn post<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> Result<T, ApiError> {
    let request = gloo_net::http::Request::post(&format!("{BASE}{path}"))
        .json(body)
        .map_err(|e| ApiError::Network(e.to_string()))?;
    let response = request
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?;
    decode(response).await
}

/// POST with a raw text body (pasted lesson result), returning a DTO.
///
/// # Errors
///
/// [`ApiError`] on network failure, API error body, or decode mismatch.
pub async fn post_text<T: DeserializeOwned>(path: &str, body: String) -> Result<T, ApiError> {
    let response = gloo_net::http::Request::post(&format!("{BASE}{path}"))
        .header("content-type", "application/json")
        .body(body)
        .map_err(|e| ApiError::Network(e.to_string()))?
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?;
    decode(response).await
}
