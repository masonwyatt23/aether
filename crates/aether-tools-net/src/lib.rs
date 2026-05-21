//! `aether-tools-net` — real HTTP and LLM tool implementations for the Aether
//! runtime. Wire these into a [`aether_eval::ToolRegistry`] via [`install`].
//!
//! # Usage
//!
//! ```rust,ignore
//! let mut rt = aether_eval::Runtime::new(module);
//! aether_tools_net::install(&mut rt.tools);
//! ```
//!
//! # Environment variables
//!
//! | Variable | Default | Purpose |
//! |---|---|---|
//! | `ANTHROPIC_API_KEY` | — | **Required** for `llm_complete`; error if unset |
//! | `AETHER_LLM_MODEL` | `claude-haiku-4-5-20251001` | Model override for `llm_complete` |
//! | `AETHER_HTTP_TIMEOUT_MS` | `10000` | Request timeout in milliseconds for `http_get` |

pub mod http;
pub mod llm;

/// Install real network-backed tool handlers into `reg`.
///
/// Registers:
/// - `http_get`     — blocking HTTP GET via `reqwest`
/// - `llm_complete` — Anthropic Messages API (requires `ANTHROPIC_API_KEY`)
///
/// Both handlers override any previously registered handler for the same name.
pub fn install(reg: &mut aether_eval::ToolRegistry) {
    reg.register("http_get", http::http_get);
    reg.register("llm_complete", llm::llm_complete);
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_eval::ToolRegistry;

    #[test]
    fn install_populates_http_get_and_llm_complete() {
        let mut reg = ToolRegistry::new();
        assert!(reg.get("http_get").is_none());
        assert!(reg.get("llm_complete").is_none());

        install(&mut reg);

        assert!(
            reg.get("http_get").is_some(),
            "http_get should be registered after install()"
        );
        assert!(
            reg.get("llm_complete").is_some(),
            "llm_complete should be registered after install()"
        );
        // Two tools registered (no spurious extras).
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn install_overwrites_existing_handlers() {
        let mut reg = ToolRegistry::new();
        // Pre-register a stub.
        reg.register("http_get", |_args| {
            let arena = aether_ast::ProvArena::new();
            Ok(aether_eval::Value::Str(
                "stub".into(),
                aether_ast::ProvChain::singleton(
                    arena,
                    aether_ast::ProvOp::Tool("http_get".into()),
                    aether_ast::Span::DUMMY,
                ),
            ))
        });
        install(&mut reg);
        // Still exactly 2 (the stub was overwritten, not duplicated).
        // llm_complete was added fresh.
        assert_eq!(reg.len(), 2);
        assert!(reg.get("http_get").is_some());
        assert!(reg.get("llm_complete").is_some());
    }

    /// Calling `llm_complete` without `ANTHROPIC_API_KEY` set must return
    /// `EvalError::User("ANTHROPIC_API_KEY unset")` — no network call made.
    #[test]
    fn llm_complete_missing_api_key_returns_user_error() {
        // Ensure the env var is absent for this test.
        std::env::remove_var("ANTHROPIC_API_KEY");

        let f = llm::llm_complete;
        let arena = aether_ast::ProvArena::new();
        let prov = aether_ast::ProvChain::singleton(
            arena,
            aether_ast::ProvOp::Lit,
            aether_ast::Span::DUMMY,
        );
        let args = [aether_eval::Value::Str("hello".into(), prov)];
        let err = f(&args).unwrap_err();
        assert!(
            err.to_string().contains("ANTHROPIC_API_KEY unset"),
            "unexpected error: {err}"
        );
    }

    /// Real network integration test — skipped by default.
    /// Run with: `cargo test -p aether-tools-net -- --ignored`
    #[test]
    #[ignore]
    fn integration_llm_complete_real_api() {
        // Requires ANTHROPIC_API_KEY to be set in the environment.
        let f = llm::llm_complete;
        let arena = aether_ast::ProvArena::new();
        let prov = aether_ast::ProvChain::singleton(
            arena,
            aether_ast::ProvOp::Lit,
            aether_ast::Span::DUMMY,
        );
        let args = [aether_eval::Value::Str(
            "Reply with the single word: pong".into(),
            prov,
        )];
        let v = f(&args).unwrap();
        let text = v.as_str().unwrap();
        assert!(!text.is_empty(), "expected non-empty response");
    }

    /// Real network integration test — skipped by default.
    #[test]
    #[ignore]
    fn integration_http_get_real_network() {
        let f = http::http_get;
        let arena = aether_ast::ProvArena::new();
        let prov = aether_ast::ProvChain::singleton(
            arena,
            aether_ast::ProvOp::Lit,
            aether_ast::Span::DUMMY,
        );
        let args = [aether_eval::Value::Str(
            "https://httpbin.org/get".into(),
            prov,
        )];
        let v = f(&args).unwrap();
        assert!(!v.as_str().unwrap().is_empty());
    }
}
