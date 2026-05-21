//! Real `http_get` implementation via `reqwest::blocking`.

use aether_ast::{ProvArena, ProvChain, ProvOp, Span};
use aether_eval::{EResult, EvalError, Value};
use std::time::Duration;

/// `http_get(url: Str) -> Str`
///
/// Performs a blocking HTTP GET request and returns the response body as a
/// `Str`. Reads `AETHER_HTTP_TIMEOUT_MS` from the environment (default 10000).
/// Network errors are translated into `EvalError::User`.
pub fn http_get(args: &[Value]) -> EResult<Value> {
    let url = args
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| EvalError::TypeError("http_get: expected Str argument".into()))?
        .to_string();

    let timeout_ms: u64 = std::env::var("AETHER_HTTP_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| EvalError::User(format!("http_get: failed to build client: {e}")))?;

    let response = client
        .get(&url)
        .send()
        .map_err(|e| EvalError::User(format!("http_get: request failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(EvalError::User(format!(
            "http_get: HTTP {status} for {url}"
        )));
    }

    let body = response
        .text()
        .map_err(|e| EvalError::User(format!("http_get: failed to read response body: {e}")))?;

    let arena = ProvArena::new();
    let prov = ProvChain::singleton(arena, ProvOp::Tool("http_get".into()), Span::DUMMY);
    Ok(Value::Str(body, prov))
}
