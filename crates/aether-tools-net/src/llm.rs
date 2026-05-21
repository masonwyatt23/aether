//! Real `llm_complete` implementation via the Anthropic Messages API.

use aether_ast::{ProvArena, ProvChain, ProvOp, Span};
use aether_eval::{EResult, EvalError, Value};
use serde::Deserialize;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
const DEFAULT_MAX_TOKENS: u32 = 256;

/// Response types for deserialization.
#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

/// `llm_complete(prompt: Str) -> Str`
///
/// Sends `prompt` to the Anthropic Messages API and returns the first text
/// block from the response.
///
/// Environment variables:
/// - `ANTHROPIC_API_KEY` — required; returns `EvalError::User` if missing.
/// - `AETHER_LLM_MODEL` — override the model (default `claude-haiku-4-5-20251001`).
pub fn llm_complete(args: &[Value]) -> EResult<Value> {
    let prompt = args
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| EvalError::TypeError("llm_complete: expected Str argument".into()))?
        .to_string();

    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| EvalError::User("ANTHROPIC_API_KEY unset".into()))?;

    let model = std::env::var("AETHER_LLM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    let body = serde_json::json!({
        "model": model,
        "max_tokens": DEFAULT_MAX_TOKENS,
        "messages": [
            {"role": "user", "content": prompt}
        ]
    });

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| EvalError::User(format!("llm_complete: request failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response.text().unwrap_or_default();
        return Err(EvalError::User(format!(
            "llm_complete: HTTP {status}: {err_body}"
        )));
    }

    let parsed: MessagesResponse = response
        .json()
        .map_err(|e| EvalError::User(format!("llm_complete: failed to parse response: {e}")))?;

    let text = parsed
        .content
        .into_iter()
        .next()
        .map(|b| b.text)
        .ok_or_else(|| EvalError::User("llm_complete: empty content array in response".into()))?;

    let arena = ProvArena::new();
    let prov = ProvChain::singleton(arena, ProvOp::Tool("llm_complete".into()), Span::DUMMY);
    Ok(Value::Str(text, prov))
}
