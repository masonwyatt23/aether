//! Tool registry: a slot for Rust-side handlers to fulfill Aether `tool`
//! declarations or override agent primitives like `llm_complete`.
//!
//! Registered tools are dispatched **before** built-in fallbacks, so a caller
//! can swap in a real LLM client at runtime without recompiling the language.

use crate::value::Value;
use crate::EResult;
use std::collections::HashMap;

/// A tool handler. Plain `fn` pointer so the registry stays `Send + Sync`
/// without trait objects or interior mutability.
pub type ToolFn = fn(args: &[Value]) -> EResult<Value>;

#[derive(Default, Clone)]
pub struct ToolRegistry {
    handlers: HashMap<String, ToolFn>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: impl Into<String>, f: ToolFn) {
        self.handlers.insert(name.into(), f);
    }

    pub fn get(&self, name: &str) -> Option<ToolFn> {
        self.handlers.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.handlers.keys().map(String::as_str)
    }
}

// ── default handlers ────────────────────────────────────────────────────────

/// Install the default agent-native tool stubs (LLM, mem, etc.). These are
/// deterministic / file-backed so tests are reproducible. Real bindings can
/// be registered over the top by the CLI or by an embedding application.
pub fn install_defaults(reg: &mut ToolRegistry) {
    reg.register("llm_complete", llm_complete_stub);
    reg.register("mem_get", mem_get_file);
    reg.register("mem_set", mem_set_file);
}

// ── persistent mem ──────────────────────────────────────────────────────────
// A JSON file under `${AETHER_MEM_PATH:-~/.aether/mem.json}`. The store is a
// flat `{ "<key>": "<value>", … }` object. `mem_get` returns the empty string
// for missing keys (avoids tripping the `Throw` effect path in client code).

fn mem_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("AETHER_MEM_PATH") {
        return std::path::PathBuf::from(p);
    }
    let mut p = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    p.push(".aether");
    p.push("mem.json");
    p
}

fn mem_load() -> std::collections::BTreeMap<String, String> {
    let path = mem_path();
    let Ok(bytes) = std::fs::read(&path) else {
        return Default::default();
    };
    parse_flat_json_object(std::str::from_utf8(&bytes).unwrap_or("")).unwrap_or_default()
}

fn mem_save(store: &std::collections::BTreeMap<String, String>) -> Result<(), std::io::Error> {
    let path = mem_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = String::from("{\n");
    let mut first = true;
    for (k, v) in store {
        if !first {
            out.push_str(",\n");
        }
        first = false;
        out.push_str("  ");
        json_escape(k, &mut out);
        out.push_str(": ");
        json_escape(v, &mut out);
    }
    out.push_str("\n}\n");
    std::fs::write(path, out)
}

fn mem_get_file(args: &[Value]) -> EResult<Value> {
    let key = args
        .first()
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let store = mem_load();
    let value = store.get(&key).cloned().unwrap_or_default();
    let arena = aether_ast::ProvArena::new();
    Ok(Value::Str(
        value,
        aether_ast::ProvChain::singleton(
            arena,
            aether_ast::ProvOp::Tool("mem_get".into()),
            aether_ast::Span::DUMMY,
        ),
    ))
}

fn mem_set_file(args: &[Value]) -> EResult<Value> {
    let key = args
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| crate::EvalError::TypeError("mem_set: key must be Str".into()))?
        .to_string();
    let value = args
        .get(1)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut store = mem_load();
    store.insert(key, value);
    mem_save(&store).map_err(|e| crate::EvalError::User(format!("mem_set: write failed: {e}")))?;
    let arena = aether_ast::ProvArena::new();
    Ok(Value::Unit(aether_ast::ProvChain::singleton(
        arena,
        aether_ast::ProvOp::Tool("mem_set".into()),
        aether_ast::Span::DUMMY,
    )))
}

// ── tiny JSON helpers (avoid serde_json as a dep here) ──────────────────────

fn json_escape(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn parse_flat_json_object(s: &str) -> Option<std::collections::BTreeMap<String, String>> {
    let s = s.trim();
    if !s.starts_with('{') || !s.ends_with('}') {
        return None;
    }
    let body = &s[1..s.len() - 1];
    let mut out = std::collections::BTreeMap::new();
    let mut chars = body.chars().peekable();
    loop {
        // skip whitespace + commas
        while matches!(chars.peek(), Some(c) if c.is_whitespace() || *c == ',') {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        if chars.peek() != Some(&'"') {
            return None;
        }
        let key = read_json_str(&mut chars)?;
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        if chars.next() != Some(':') {
            return None;
        }
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        if chars.peek() != Some(&'"') {
            return None;
        }
        let val = read_json_str(&mut chars)?;
        out.insert(key, val);
    }
    Some(out)
}

fn read_json_str(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    if chars.next() != Some('"') {
        return None;
    }
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            },
            c => out.push(c),
        }
    }
}

/// Deterministic LLM completion stub. Hashes the prompt and returns a short
/// `Str` so tests don't need a network. Real handlers replace this.
fn llm_complete_stub(args: &[Value]) -> EResult<Value> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let prompt = args.first().and_then(Value::as_str).unwrap_or("");
    let mut h = DefaultHasher::new();
    prompt.hash(&mut h);
    let digest = h.finish();
    // Mimic an LLM reply by echoing a short tag.
    let reply = format!("<llm-stub:{digest:016x}> ack");
    let arena = aether_ast::ProvArena::new();
    let prov = aether_ast::ProvChain::singleton(
        arena,
        aether_ast::ProvOp::Tool("llm_complete".into()),
        aether_ast::Span::DUMMY,
    );
    Ok(Value::Str(reply, prov))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_basic() {
        let mut r = ToolRegistry::new();
        assert!(r.is_empty());
        r.register("echo", |args| {
            let s = args
                .first()
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let arena = aether_ast::ProvArena::new();
            Ok(Value::Str(
                s,
                aether_ast::ProvChain::singleton(
                    arena,
                    aether_ast::ProvOp::Tool("echo".into()),
                    aether_ast::Span::DUMMY,
                ),
            ))
        });
        assert_eq!(r.len(), 1);
        assert!(r.get("echo").is_some());
        assert!(r.get("nope").is_none());
    }

    #[test]
    fn mem_roundtrip_via_env_override() {
        // Use a per-pid temp file so tests in the same workspace don't collide.
        let tmp = std::env::temp_dir().join(format!("aether_mem_test_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        std::env::set_var("AETHER_MEM_PATH", &tmp);

        let arena = aether_ast::ProvArena::new();
        let prov = aether_ast::ProvChain::singleton(
            arena,
            aether_ast::ProvOp::Lit,
            aether_ast::Span::DUMMY,
        );
        let key = Value::Str("greeting".into(), prov.clone());
        let val = Value::Str("hello, world".into(), prov.clone());

        // First read: missing key returns empty string.
        let r0 = mem_get_file(&[key.clone()]).unwrap();
        assert_eq!(r0.as_str(), Some(""));

        // Write then read.
        let _ = mem_set_file(&[key.clone(), val.clone()]).unwrap();
        let r1 = mem_get_file(&[key.clone()]).unwrap();
        assert_eq!(r1.as_str(), Some("hello, world"));

        std::env::remove_var("AETHER_MEM_PATH");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn default_llm_stub_deterministic() {
        let mut r = ToolRegistry::new();
        install_defaults(&mut r);
        let f = r.get("llm_complete").unwrap();
        let arena = aether_ast::ProvArena::new();
        let prov = aether_ast::ProvChain::singleton(
            arena,
            aether_ast::ProvOp::Lit,
            aether_ast::Span::DUMMY,
        );
        let a = f(&[Value::Str("hello".into(), prov.clone())]).unwrap();
        let b = f(&[Value::Str("hello".into(), prov)]).unwrap();
        assert_eq!(a.as_str(), b.as_str());
        assert!(a.as_str().unwrap().contains("llm-stub"));
    }
}
