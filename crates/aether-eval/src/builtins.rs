//! Built-in functions reachable from any Aether program.
//!
//! Agent primitives (`introspect`, `summarize`, `provenance`) live here in
//! their minimal form; the richer stdlib (std.plan, std.iter, std.mem) lives
//! in the `aether-stdlib` crate which is wired in by the CLI.

use crate::value::Value;
use crate::{EResult, EvalError, Runtime};
use aether_ast::*;
use regex::Regex;
use serde_json;

/// A summary of a module / scope returned by `introspect(...)`.
#[derive(Debug, Clone)]
pub struct ModuleSurface {
    pub name: String,
    pub doc: Option<String>,
    pub exports: Vec<ExportEntry>,
}

#[derive(Debug, Clone)]
pub struct ExportEntry {
    pub name: String,
    pub kind: &'static str, // "fn" | "tool" | "let" | "type"
    pub signature: String,
    pub effects: Vec<String>,
    pub doc: Option<String>,
}

impl ModuleSurface {
    pub fn format(&self) -> String {
        let mut s = String::new();
        s.push_str("module ");
        s.push_str(&self.name);
        if let Some(d) = &self.doc {
            s.push_str(" — ");
            s.push_str(d);
        }
        s.push('\n');
        for e in &self.exports {
            s.push_str("  ");
            s.push_str(e.kind);
            s.push(' ');
            s.push_str(&e.name);
            s.push_str(" : ");
            s.push_str(&e.signature);
            if !e.effects.is_empty() {
                s.push_str(" !{");
                s.push_str(&e.effects.join(","));
                s.push('}');
            }
            if let Some(doc) = &e.doc {
                s.push_str("   # ");
                let line = doc.lines().next().unwrap_or("");
                s.push_str(line);
            }
            s.push('\n');
        }
        s
    }
}

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Process-local in-memory cache backing std::cache.
static CACHE_STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

pub fn dispatch(
    rt: &mut Runtime,
    name: &str,
    args: &[Value],
    span: Span,
) -> EResult<Option<Value>> {
    // Tool registry has priority over built-ins so a host application can swap
    // in a real implementation for any agent-native primitive.
    if let Some(f) = rt.tools.get(name) {
        let v = f(args)?;
        let prov = ProvChain::extend(
            rt.arena.clone(),
            ProvOp::Tool(name.to_string()),
            span,
            args.iter().map(|a| a.prov().head).collect(),
        );
        return Ok(Some(v.with_prov(prov)));
    }
    let v = match name {
        "assert" => {
            let cond = args
                .first()
                .and_then(Value::as_bool)
                .ok_or_else(|| EvalError::TypeError("assert(): expected Bool".into()))?;
            if !cond {
                return Err(EvalError::User("assertion failed".into()));
            }
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("assert".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "assert_eq" => {
            let a = args
                .first()
                .ok_or_else(|| EvalError::TypeError("assert_eq(): missing a".into()))?;
            let b = args
                .get(1)
                .ok_or_else(|| EvalError::TypeError("assert_eq(): missing b".into()))?;
            if !a.eq_val(b) {
                return Err(EvalError::User(format!(
                    "assert_eq failed: {} != {}",
                    a.display(),
                    b.display()
                )));
            }
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("assert_eq".into()),
                span,
                args.iter().map(|x| x.prov().head).collect(),
            ))
        }
        "snap_expect" => {
            let label = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("snap_expect(): first arg must be Str (label)".into())
                })?
                .to_string();
            let value = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("snap_expect(): second arg must be Str (value)".into())
                })?
                .to_string();
            rt.snapshot_buffer.push((label, value));
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("snap_expect".into()),
                span,
                args.iter().map(|x| x.prov().head).collect(),
            ))
        }
        "print" | "println" => {
            let s = args.get(0).map(Value::display).unwrap_or_default();
            if !rt.capture_only {
                if name == "println" || !s.ends_with('\n') {
                    println!("{s}");
                } else {
                    print!("{s}");
                }
            }
            rt.stdout.push_str(&s);
            rt.stdout.push('\n');
            let prov = ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call(name.into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            );
            Value::Unit(prov)
        }
        "str" => {
            let s = args.get(0).map(Value::display).unwrap_or_default();
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("str".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "int" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("int(): expected Str".into()))?;
            let n: i64 = s
                .trim()
                .parse()
                .map_err(|_| EvalError::User(format!("int(): not a number: {s:?}")))?;
            Value::Int(
                n,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("int".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "len" => {
            let n = match args.get(0) {
                Some(Value::Str(s, _)) => s.chars().count() as i64,
                Some(Value::List(items, _)) => items.len() as i64,
                _ => return Err(EvalError::TypeError("len(): expected Str or List".into())),
            };
            Value::Int(
                n,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("len".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "abs" => {
            let n = args
                .first()
                .and_then(Value::as_int)
                .ok_or_else(|| EvalError::TypeError("abs(): Int".into()))?;
            Value::Int(
                n.abs(),
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("abs".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "max" => {
            let a = args
                .first()
                .and_then(Value::as_int)
                .ok_or_else(|| EvalError::TypeError("max(): Int".into()))?;
            let b = args
                .get(1)
                .and_then(Value::as_int)
                .ok_or_else(|| EvalError::TypeError("max(): Int".into()))?;
            Value::Int(
                a.max(b),
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("max".into()),
                    span,
                    args.iter().map(|x| x.prov().head).collect(),
                ),
            )
        }
        "min" => {
            let a = args
                .first()
                .and_then(Value::as_int)
                .ok_or_else(|| EvalError::TypeError("min(): Int".into()))?;
            let b = args
                .get(1)
                .and_then(Value::as_int)
                .ok_or_else(|| EvalError::TypeError("min(): Int".into()))?;
            Value::Int(
                a.min(b),
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("min".into()),
                    span,
                    args.iter().map(|x| x.prov().head).collect(),
                ),
            )
        }
        "introspect" => {
            let target = args
                .first()
                .and_then(Value::as_str)
                .unwrap_or("current")
                .to_string();
            let surface = build_surface(rt, &target);
            Value::ModuleSurface(
                surface,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("introspect".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "print_module_surface" => {
            if let Some(Value::ModuleSurface(ms, _)) = args.first() {
                let s = ms.format();
                if !rt.capture_only {
                    print!("{s}");
                }
                rt.stdout.push_str(&s);
            }
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("print_module_surface".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "summarize" => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let scope = args
                .first()
                .and_then(Value::as_str)
                .unwrap_or("current")
                .to_string();
            let budget = args.get(1).and_then(Value::as_int).unwrap_or(160);
            // Cache key = hash(scope, budget, module_decl_count).
            // Content-hash uses decl count as a coarse fingerprint; for a tighter
            // invalidation key, hash each decl's name+span — kept coarse here to
            // stay fast and avoid traversing the entire AST per call.
            let mut h = DefaultHasher::new();
            scope.hash(&mut h);
            budget.hash(&mut h);
            rt.module.decls.len().hash(&mut h);
            let key = h.finish();
            let summary = if let Some(cached) = rt.summarize_cache.get(&key) {
                cached.clone()
            } else {
                let s = summarize_scope(rt, &scope, budget as usize);
                rt.summarize_cache.insert(key, s.clone());
                s
            };
            Value::Str(
                summary,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("summarize".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "provenance" => {
            // Return a first-class ProvHandle referring to the *argument's* chain.
            let chain = args
                .first()
                .map(|v| v.prov().clone())
                .ok_or_else(|| EvalError::TypeError("provenance(): missing argument".into()))?;
            let outer = ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("provenance".into()),
                span,
                vec![chain.head],
            );
            Value::ProvHandle(chain, outer)
        }
        "print_prov" => {
            if let Some(Value::ProvHandle(chain, _)) = args.first() {
                let s = format_prov(chain);
                if !rt.capture_only {
                    print!("{s}");
                }
                rt.stdout.push_str(&s);
            }
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("print_prov".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "http_get" => {
            // Stub: return a deterministic placeholder. Real interpreter could shell out.
            let url = args.first().and_then(Value::as_str).unwrap_or("");
            let body = format!("<aether stub: GET {url}>");
            Value::Str(
                body,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Tool("http_get".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "iter_refine" => {
            // iter_refine(seed, step_fn_name, budget)
            let seed = args
                .first()
                .cloned()
                .ok_or_else(|| EvalError::TypeError("iter_refine: missing seed".into()))?;
            let step_name = match args.get(1) {
                Some(Value::Fn(n, _)) => n.clone(),
                Some(Value::Str(s, _)) => s.clone(),
                _ => {
                    return Err(EvalError::TypeError(
                        "iter_refine: step must be a fn reference or name".into(),
                    ))
                }
            };
            let budget = args.get(2).and_then(Value::as_int).unwrap_or(1);
            let mut cur = seed;
            for _ in 0..budget {
                cur = call_named(rt, &step_name, vec![cur.clone()], span)?;
            }
            let prov = ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("iter_refine".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            );
            cur.with_prov(prov)
        }
        // ── std::list natives ──────────────────────────────────────────────
        "list_sum_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| EvalError::TypeError("list_sum_native(): expected [Int]".into()))?;
            let sum: i64 = items
                .iter()
                .map(|v| {
                    v.as_int().ok_or_else(|| {
                        EvalError::TypeError("list_sum_native(): list element is not Int".into())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .sum();
            Value::Int(
                sum,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("list_sum_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "list_max_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| EvalError::TypeError("list_max_native(): expected [Int]".into()))?;
            if items.is_empty() {
                return Err(EvalError::User(
                    "list_max_native(): cannot take max of empty list".into(),
                ));
            }
            let m = items
                .iter()
                .map(|v| {
                    v.as_int().ok_or_else(|| {
                        EvalError::TypeError("list_max_native(): list element is not Int".into())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .max()
                .unwrap();
            Value::Int(
                m,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("list_max_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "list_contains_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("list_contains_native(): expected [Int]".into())
                })?;
            let x = args.get(1).and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("list_contains_native(): expected Int needle".into())
            })?;
            let found = items.iter().any(|v| v.as_int() == Some(x));
            Value::Bool(
                found,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("list_contains_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "list_count_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("list_count_native(): expected [Int]".into())
                })?;
            let x = args.get(1).and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("list_count_native(): expected Int needle".into())
            })?;
            let count = items.iter().filter(|v| v.as_int() == Some(x)).count() as i64;
            Value::Int(
                count,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("list_count_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "list_reversed_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("list_reversed_native(): expected [Int]".into())
                })?;
            let mut rev = items.clone();
            rev.reverse();
            Value::List(
                rev,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("list_reversed_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::strlist natives ──────────────────────────────────────────────
        "strlist_len_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("strlist_len_native(): expected [Str]".into())
                })?;
            Value::Int(
                items.len() as i64,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("strlist_len_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "strlist_get_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("strlist_get_native(): expected [Str]".into())
                })?;
            let i = args.get(1).and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("strlist_get_native(): expected Int index".into())
            })?;
            let item = items.get(i as usize).ok_or_else(|| {
                EvalError::User(format!(
                    "strlist_get_native(): index {i} out of bounds (len={})",
                    items.len()
                ))
            })?;
            let s = item
                .as_str()
                .ok_or_else(|| {
                    EvalError::TypeError("strlist_get_native(): list element is not Str".into())
                })?
                .to_string();
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("strlist_get_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "strlist_join_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("strlist_join_native(): expected [Str]".into())
                })?;
            let sep = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("strlist_join_native(): expected Str sep".into())
            })?;
            let parts: Vec<&str> = items
                .iter()
                .map(|v| {
                    v.as_str().ok_or_else(|| {
                        EvalError::TypeError(
                            "strlist_join_native(): list element is not Str".into(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let joined = parts.join(sep);
            Value::Str(
                joined,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("strlist_join_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "strlist_contains_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("strlist_contains_native(): expected [Str]".into())
                })?;
            let needle = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("strlist_contains_native(): expected Str needle".into())
            })?;
            let found = items.iter().any(|v| v.as_str() == Some(needle));
            Value::Bool(
                found,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("strlist_contains_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "strlist_reversed_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("strlist_reversed_native(): expected [Str]".into())
                })?;
            let mut rev = items.clone();
            rev.reverse();
            Value::List(
                rev,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("strlist_reversed_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "strlist_head_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("strlist_head_native(): expected [Str]".into())
                })?;
            let first = items.first().ok_or_else(|| {
                EvalError::User("strlist_head_native(): cannot take head of empty list".into())
            })?;
            let s = first
                .as_str()
                .ok_or_else(|| {
                    EvalError::TypeError("strlist_head_native(): list element is not Str".into())
                })?
                .to_string();
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("strlist_head_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "strlist_tail_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("strlist_tail_native(): expected [Str]".into())
                })?;
            let tail = if items.is_empty() {
                vec![]
            } else {
                items[1..].to_vec()
            };
            Value::List(
                tail,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("strlist_tail_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::string natives ───────────────────────────────────────────────
        "str_upper_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("str_upper_native(): expected Str".into()))?
                .to_uppercase();
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("str_upper_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "str_lower_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("str_lower_native(): expected Str".into()))?
                .to_lowercase();
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("str_lower_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "str_contains_native" => {
            let s = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("str_contains_native(): expected Str".into())
            })?;
            let needle = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("str_contains_native(): expected Str needle".into())
            })?;
            let found = s.contains(needle);
            Value::Bool(
                found,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("str_contains_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "str_starts_with_native" => {
            let s = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("str_starts_with_native(): expected Str".into())
            })?;
            let prefix = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("str_starts_with_native(): expected Str prefix".into())
            })?;
            let result = s.starts_with(prefix);
            Value::Bool(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("str_starts_with_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "str_split_on_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("str_split_on_native(): expected Str".into()))?
                .to_string();
            let sep = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("str_split_on_native(): expected Str sep".into())
                })?
                .to_string();
            let prov_inner = ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("str_split_on_native".into()),
                span,
            );
            let parts: Vec<Value> = if sep.is_empty() {
                s.chars()
                    .map(|c| Value::Str(c.to_string(), prov_inner.clone()))
                    .collect()
            } else {
                s.split(&sep as &str)
                    .map(|p| Value::Str(p.to_string(), prov_inner.clone()))
                    .collect()
            };
            Value::List(
                parts,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("str_split_on_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "str_join_native" => {
            let items = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| EvalError::TypeError("str_join_native(): expected [Str]".into()))?;
            let sep = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("str_join_native(): expected Str sep".into())
            })?;
            let parts: Vec<&str> = items
                .iter()
                .map(|v| {
                    v.as_str().ok_or_else(|| {
                        EvalError::TypeError("str_join_native(): list element is not Str".into())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let joined = parts.join(sep);
            Value::Str(
                joined,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("str_join_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "str_trim_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("str_trim_native(): expected Str".into()))?
                .trim()
                .to_string();
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("str_trim_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::map natives ──────────────────────────────────────────────────
        "map_get_native" => {
            let entries = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("map_get_native(): expected [MapEntry]".into())
                })?;
            let key = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("map_get_native(): expected Str key".into()))?;
            for entry in entries {
                if let Value::Record(fields, _) = entry {
                    let k = fields
                        .iter()
                        .find(|(n, _)| n == "key")
                        .and_then(|(_, v)| v.as_str());
                    if k == Some(key) {
                        let val = fields
                            .iter()
                            .find(|(n, _)| n == "value")
                            .and_then(|(_, v)| v.as_int())
                            .ok_or_else(|| {
                                EvalError::TypeError(
                                    "map_get_native(): entry value is not Int".into(),
                                )
                            })?;
                        return Ok(Some(Value::Int(
                            val,
                            ProvChain::extend(
                                rt.arena.clone(),
                                ProvOp::Call("map_get_native".into()),
                                span,
                                args.iter().map(|a| a.prov().head).collect(),
                            ),
                        )));
                    }
                }
            }
            return Err(EvalError::User(format!(
                "map_get_native(): key not found: {key:?}"
            )));
        }
        "map_set_native" => {
            let entries = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("map_set_native(): expected [MapEntry]".into())
                })?
                .clone();
            let key = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("map_set_native(): expected Str key".into()))?
                .to_string();
            let value = args.get(2).and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("map_set_native(): expected Int value".into())
            })?;
            let prov_entry = ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("map_set_native".into()),
                span,
            );
            let new_entry = Value::Record(
                vec![
                    (
                        "key".to_string(),
                        Value::Str(key.clone(), prov_entry.clone()),
                    ),
                    ("value".to_string(), Value::Int(value, prov_entry.clone())),
                ],
                prov_entry,
            );
            // Replace existing or append.
            let mut result: Vec<Value> = entries
                .into_iter()
                .filter(|e| {
                    if let Value::Record(fields, _) = e {
                        fields
                            .iter()
                            .find(|(n, _)| n == "key")
                            .and_then(|(_, v)| v.as_str())
                            .map(|k| k != key.as_str())
                            .unwrap_or(true)
                    } else {
                        true
                    }
                })
                .collect();
            result.push(new_entry);
            Value::List(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("map_set_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "map_has_native" => {
            let entries = args
                .first()
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("map_has_native(): expected [MapEntry]".into())
                })?;
            let key = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("map_has_native(): expected Str key".into()))?;
            let found = entries.iter().any(|e| {
                if let Value::Record(fields, _) = e {
                    fields
                        .iter()
                        .find(|(n, _)| n == "key")
                        .and_then(|(_, v)| v.as_str())
                        == Some(key)
                } else {
                    false
                }
            });
            Value::Bool(
                found,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("map_has_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::path natives ─────────────────────────────────────────────────
        "path_join_native" => {
            let a = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("path_join_native(): expected Str a".into()))?;
            let b = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("path_join_native(): expected Str b".into()))?;
            let joined = if b.is_empty() {
                a.to_string()
            } else if a.is_empty() {
                b.to_string()
            } else {
                std::path::Path::new(a)
                    .join(b)
                    .to_string_lossy()
                    .into_owned()
            };
            Value::Str(
                joined,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("path_join_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "path_basename_native" => {
            let p = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("path_basename_native(): expected Str".into())
            })?;
            let base = std::path::Path::new(p)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            Value::Str(
                base,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("path_basename_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "path_dirname_native" => {
            let p = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("path_dirname_native(): expected Str".into())
            })?;
            let dir = std::path::Path::new(p)
                .parent()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| ".".into());
            Value::Str(
                dir,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("path_dirname_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "path_extension_native" => {
            let p = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("path_extension_native(): expected Str".into())
            })?;
            let ext = std::path::Path::new(p)
                .extension()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            Value::Str(
                ext,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("path_extension_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "path_exists_native" => {
            let p = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("path_exists_native(): expected Str".into()))?;
            let exists = std::path::Path::new(p).exists();
            Value::Bool(
                exists,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("path_exists_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::time natives ──────────────────────────────────────────────────
        "time_now_ms_native" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Value::Int(
                ms,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("time_now_ms_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "time_monotonic_ms_native" => {
            use std::time::Instant;
            // We rely on a process-level lazy to get a consistent epoch.
            use std::sync::OnceLock;
            static EPOCH: OnceLock<Instant> = OnceLock::new();
            let epoch = EPOCH.get_or_init(Instant::now);
            let ms = epoch.elapsed().as_millis() as i64;
            Value::Int(
                ms,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("time_monotonic_ms_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "time_format_iso_native" => {
            let ms = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("time_format_iso_native(): expected Int".into())
            })?;
            let s = format_iso_from_ms(ms);
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("time_format_iso_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::env natives ───────────────────────────────────────────────────
        "env_get_native" => {
            let name = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("env_get_native(): expected Str name".into())
            })?;
            let val = std::env::var(name).unwrap_or_default();
            Value::Str(
                val,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("env_get_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "env_has_native" => {
            let name = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("env_has_native(): expected Str name".into())
            })?;
            let has = std::env::var_os(name).is_some();
            Value::Bool(
                has,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("env_has_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "env_set_native" => {
            let name = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("env_set_native(): expected Str name".into()))?
                .to_string();
            let value = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("env_set_native(): expected Str value".into()))?
                .to_string();
            #[allow(deprecated)]
            std::env::set_var(&name, &value);
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("env_set_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }

        // ── std::fmt natives ───────────────────────────────────────────────────
        "fmt1_native" => {
            let template = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt1_native(): expected Str template".into()))?
                .to_string();
            let a = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt1_native(): expected Str a".into()))?;
            let result = template.replacen("{}", a, 1);
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("fmt1_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "fmt2_native" => {
            let template = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt2_native(): expected Str template".into()))?
                .to_string();
            let a = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt2_native(): expected Str a".into()))?;
            let b = args
                .get(2)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt2_native(): expected Str b".into()))?;
            let result = template.replacen("{}", a, 1).replacen("{}", b, 1);
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("fmt2_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "fmt3_native" => {
            let template = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt3_native(): expected Str template".into()))?
                .to_string();
            let a = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt3_native(): expected Str a".into()))?;
            let b = args
                .get(2)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt3_native(): expected Str b".into()))?;
            let c = args
                .get(3)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt3_native(): expected Str c".into()))?;
            let result = template
                .replacen("{}", a, 1)
                .replacen("{}", b, 1)
                .replacen("{}", c, 1);
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("fmt3_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "fmt4_native" => {
            let template = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt4_native(): expected Str template".into()))?
                .to_string();
            let a = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt4_native(): expected Str a".into()))?;
            let b = args
                .get(2)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt4_native(): expected Str b".into()))?;
            let c = args
                .get(3)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt4_native(): expected Str c".into()))?;
            let d = args
                .get(4)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt4_native(): expected Str d".into()))?;
            let result = template
                .replacen("{}", a, 1)
                .replacen("{}", b, 1)
                .replacen("{}", c, 1)
                .replacen("{}", d, 1);
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("fmt4_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "fmt5_native" => {
            let template = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt5_native(): expected Str template".into()))?
                .to_string();
            let a = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt5_native(): expected Str a".into()))?;
            let b = args
                .get(2)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt5_native(): expected Str b".into()))?;
            let c = args
                .get(3)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt5_native(): expected Str c".into()))?;
            let d = args
                .get(4)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt5_native(): expected Str d".into()))?;
            let e = args
                .get(5)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fmt5_native(): expected Str e".into()))?;
            let result = template
                .replacen("{}", a, 1)
                .replacen("{}", b, 1)
                .replacen("{}", c, 1)
                .replacen("{}", d, 1)
                .replacen("{}", e, 1);
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("fmt5_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "fmt_list_native" => {
            let template = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("fmt_list_native(): expected Str template".into())
                })?
                .to_string();
            let items = args
                .get(1)
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("fmt_list_native(): expected [Str] args".into())
                })?;
            let mut result = template;
            for item in items {
                let s = item.as_str().ok_or_else(|| {
                    EvalError::TypeError("fmt_list_native(): list element is not Str".into())
                })?;
                result = result.replacen("{}", s, 1);
            }
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("fmt_list_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "pad_left_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("pad_left_native(): expected Str s".into()))?
                .to_string();
            let n =
                args.get(1).and_then(Value::as_int).ok_or_else(|| {
                    EvalError::TypeError("pad_left_native(): expected Int n".into())
                })? as usize;
            let ch_str = args
                .get(2)
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("pad_left_native(): expected Str ch".into()))?;
            let ch = ch_str.chars().next().unwrap_or(' ');
            let cur_len = s.chars().count();
            let result = if cur_len >= n {
                s
            } else {
                let pad: String = std::iter::repeat(ch).take(n - cur_len).collect();
                format!("{pad}{s}")
            };
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("pad_left_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "pad_right_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("pad_right_native(): expected Str s".into()))?
                .to_string();
            let n =
                args.get(1).and_then(Value::as_int).ok_or_else(|| {
                    EvalError::TypeError("pad_right_native(): expected Int n".into())
                })? as usize;
            let ch_str = args.get(2).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("pad_right_native(): expected Str ch".into())
            })?;
            let ch = ch_str.chars().next().unwrap_or(' ');
            let cur_len = s.chars().count();
            let result = if cur_len >= n {
                s
            } else {
                let pad: String = std::iter::repeat(ch).take(n - cur_len).collect();
                format!("{s}{pad}")
            };
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("pad_right_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::result natives ────────────────────────────────────────────────
        "result_throw_native" => {
            let msg = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("result_throw_native(): expected Str msg".into())
                })?
                .to_string();
            return Err(EvalError::User(msg));
        }

        // ── std::regex natives ─────────────────────────────────────────────────
        "regex_match_native" => {
            let pattern = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_match_native(): expected Str pattern".into())
            })?;
            let input = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_match_native(): expected Str input".into())
            })?;
            let re = Regex::new(pattern).map_err(|e| {
                EvalError::User(format!(
                    "regex_match_native(): invalid pattern {pattern:?}: {e}"
                ))
            })?;
            let found = re.is_match(input);
            Value::Bool(
                found,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("regex_match_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "regex_find_native" => {
            let pattern = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_find_native(): expected Str pattern".into())
            })?;
            let input = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_find_native(): expected Str input".into())
            })?;
            let re = Regex::new(pattern).map_err(|e| {
                EvalError::User(format!(
                    "regex_find_native(): invalid pattern {pattern:?}: {e}"
                ))
            })?;
            let found = re.find(input).map(|m| m.as_str()).unwrap_or("").to_string();
            Value::Str(
                found,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("regex_find_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "regex_replace_native" => {
            let pattern = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_replace_native(): expected Str pattern".into())
            })?;
            let input = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_replace_native(): expected Str input".into())
            })?;
            let replacement = args.get(2).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_replace_native(): expected Str replacement".into())
            })?;
            let re = Regex::new(pattern).map_err(|e| {
                EvalError::User(format!(
                    "regex_replace_native(): invalid pattern {pattern:?}: {e}"
                ))
            })?;
            let result = re.replacen(input, 1, replacement).into_owned();
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("regex_replace_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "regex_split_native" => {
            let pattern = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_split_native(): expected Str pattern".into())
            })?;
            let input = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_split_native(): expected Str input".into())
            })?;
            let re = Regex::new(pattern).map_err(|e| {
                EvalError::User(format!(
                    "regex_split_native(): invalid pattern {pattern:?}: {e}"
                ))
            })?;
            let prov_inner = ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("regex_split_native".into()),
                span,
            );
            let parts: Vec<Value> = re
                .split(input)
                .map(|p| Value::Str(p.to_string(), prov_inner.clone()))
                .collect();
            Value::List(
                parts,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("regex_split_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "regex_captures_native" => {
            let pattern = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_captures_native(): expected Str pattern".into())
            })?;
            let input = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_captures_native(): expected Str input".into())
            })?;
            let re = Regex::new(pattern).map_err(|e| {
                EvalError::User(format!(
                    "regex_captures_native(): invalid pattern {pattern:?}: {e}"
                ))
            })?;
            let prov_inner = ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("regex_captures_native".into()),
                span,
            );
            let parts: Vec<Value> = re
                .captures(input)
                .map(|caps| {
                    // skip group 0 (whole match), return explicit capture groups
                    (1..caps.len())
                        .map(|i| {
                            Value::Str(
                                caps.get(i).map(|m| m.as_str()).unwrap_or("").to_string(),
                                prov_inner.clone(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            Value::List(
                parts,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("regex_captures_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        "regex_replace_all_native" => {
            let pattern = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_replace_all_native(): expected Str pattern".into())
            })?;
            let input = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_replace_all_native(): expected Str input".into())
            })?;
            let replacement = args.get(2).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("regex_replace_all_native(): expected Str replacement".into())
            })?;
            let re = Regex::new(pattern).map_err(|e| {
                EvalError::User(format!(
                    "regex_replace_all_native(): invalid pattern {pattern:?}: {e}"
                ))
            })?;
            let result = re.replace_all(input, replacement).into_owned();
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("regex_replace_all_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::sys natives ───────────────────────────────────────────────────
        "sys_exit_native" => {
            let code = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("sys_exit_native(): expected Int code".into())
            })?;
            std::process::exit(code as i32);
        }
        "sys_args_native" => {
            // Return all args past the binary name (argv[1..]).
            let prov_inner = ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("sys_args_native".into()),
                span,
            );
            let parts: Vec<Value> = std::env::args()
                .skip(1)
                .map(|a| Value::Str(a, prov_inner.clone()))
                .collect();
            Value::List(
                parts,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("sys_args_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "sys_stdin_line_native" => {
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            // Strip the trailing newline, if any.
            if line.ends_with('\n') {
                line.pop();
            }
            if line.ends_with('\r') {
                line.pop();
            }
            Value::Str(
                line,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("sys_stdin_line_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "sys_now_unix_native" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            Value::Int(
                secs,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("sys_now_unix_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "sys_spawn_native" => {
            let cmd = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("sys_spawn_native(): expected Str cmd".into()))?
                .to_string();
            let argv = args
                .get(1)
                .and_then(|v| {
                    if let Value::List(l, _) = v {
                        Some(l)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    EvalError::TypeError("sys_spawn_native(): expected [Str] args".into())
                })?;
            let argv_strs: Vec<String> = argv
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| {
                            EvalError::TypeError(
                                "sys_spawn_native(): arg list element is not Str".into(),
                            )
                        })
                        .map(|s| s.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let output = std::process::Command::new(&cmd)
                .args(&argv_strs)
                .output()
                .map_err(|e| {
                    EvalError::User(format!("sys_spawn_native(): failed to start {cmd:?}: {e}"))
                })?;
            if !output.status.success() {
                let code = output.status.code().unwrap_or(-1);
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                return Err(EvalError::User(format!(
                    "sys_spawn_native(): {cmd:?} exited with code {code}: {stderr}"
                )));
            }
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            Value::Str(
                stdout,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("sys_spawn_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "sys_hostname_native" => {
            let hostname = std::fs::read_to_string("/etc/hostname")
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| {
                    // Fallback: try `hostname` command.
                    std::process::Command::new("hostname")
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_else(|_| "unknown".to_string())
                });
            Value::Str(
                hostname,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("sys_hostname_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        "sys_spawn_thread_sleep_native" => {
            let ms = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("sys_spawn_thread_sleep_native(): expected Int ms".into())
            })?;
            let duration = std::time::Duration::from_millis(ms as u64);
            std::thread::spawn(move || {
                std::thread::sleep(duration);
            });
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("sys_spawn_thread_sleep_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }

        // ── std::math natives ──────────────────────────────────────────────────
        "math_pow_native" => {
            let base = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("math_pow_native(): expected Int base".into())
            })?;
            let exp = args.get(1).and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("math_pow_native(): expected Int exp".into())
            })?;
            let result = if exp < 0 { 0i64 } else { base.pow(exp as u32) };
            Value::Int(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("math_pow_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "math_sqrt_native" => {
            let x = args
                .first()
                .and_then(Value::as_float)
                .ok_or_else(|| EvalError::TypeError("math_sqrt_native(): expected Float".into()))?;
            let result = if x < 0.0 { 0.0f64 } else { x.sqrt() };
            Value::Float(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("math_sqrt_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "math_floor_native" => {
            let x = args.first().and_then(Value::as_float).ok_or_else(|| {
                EvalError::TypeError("math_floor_native(): expected Float".into())
            })?;
            Value::Int(
                x.floor() as i64,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("math_floor_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "math_ceil_native" => {
            let x = args
                .first()
                .and_then(Value::as_float)
                .ok_or_else(|| EvalError::TypeError("math_ceil_native(): expected Float".into()))?;
            Value::Int(
                x.ceil() as i64,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("math_ceil_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "math_round_native" => {
            let x = args.first().and_then(Value::as_float).ok_or_else(|| {
                EvalError::TypeError("math_round_native(): expected Float".into())
            })?;
            Value::Int(
                x.round() as i64,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("math_round_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "math_min_f_native" => {
            let a = args.first().and_then(Value::as_float).ok_or_else(|| {
                EvalError::TypeError("math_min_f_native(): expected Float a".into())
            })?;
            let b = args.get(1).and_then(Value::as_float).ok_or_else(|| {
                EvalError::TypeError("math_min_f_native(): expected Float b".into())
            })?;
            Value::Float(
                a.min(b),
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("math_min_f_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "math_max_f_native" => {
            let a = args.first().and_then(Value::as_float).ok_or_else(|| {
                EvalError::TypeError("math_max_f_native(): expected Float a".into())
            })?;
            let b = args.get(1).and_then(Value::as_float).ok_or_else(|| {
                EvalError::TypeError("math_max_f_native(): expected Float b".into())
            })?;
            Value::Float(
                a.max(b),
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("math_max_f_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "math_pi_native" => Value::Float(
            std::f64::consts::PI,
            ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("math_pi_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ),
        ),

        // ── std::base64 natives ────────────────────────────────────────────────
        "base64_encode_native" => {
            let input = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("base64_encode_native(): expected Str".into())
            })?;
            let encoded = base64_encode_impl(input.as_bytes());
            Value::Str(
                encoded,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("base64_encode_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "base64_decode_native" => {
            let input = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("base64_decode_native(): expected Str".into())
            })?;
            let decoded = base64_decode_impl(input)
                .map_err(|e| EvalError::User(format!("base64_decode: {e}")))?;
            // Treat decoded bytes as UTF-8; fall back to lossy if non-UTF-8
            let s = String::from_utf8(decoded).map_err(|_| {
                EvalError::User("base64_decode: decoded bytes are not valid UTF-8".into())
            })?;
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("base64_decode_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::hash natives ──────────────────────────────────────────────────
        "hash_default_native" => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let s = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("hash_default_native(): expected Str".into())
            })?;
            let mut h = DefaultHasher::new();
            s.hash(&mut h);
            let n = h.finish() as i64;
            Value::Int(
                n,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("hash_default_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "hash_sha256_native" => {
            use sha2::{Digest, Sha256};
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("hash_sha256_native(): expected Str".into()))?;
            let hash = Sha256::digest(s.as_bytes());
            let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
            Value::Str(
                hex,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("hash_sha256_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::uuid natives ──────────────────────────────────────────────────
        "uuid_v4_native" => {
            let u = uuid_v4_impl().map_err(|e| EvalError::User(format!("uuid_v4: {e}")))?;
            Value::Str(
                u,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("uuid_v4_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "uuid_short_native" => {
            let u = uuid_v4_impl().map_err(|e| EvalError::User(format!("uuid_short: {e}")))?;
            // first 8 chars are the first hex group
            let short = u[..8].to_string();
            Value::Str(
                short,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("uuid_short_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::random natives ────────────────────────────────────────────────
        "random_int_native" => {
            let lo = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("random_int_native(): expected Int lo".into())
            })?;
            let hi = args.get(1).and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("random_int_native(): expected Int hi".into())
            })?;
            if hi < lo {
                return Err(EvalError::User(format!(
                    "random_int: hi ({hi}) < lo ({lo})"
                )));
            }
            let n =
                random_int_impl(lo, hi).map_err(|e| EvalError::User(format!("random_int: {e}")))?;
            Value::Int(
                n,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("random_int_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "random_bool_native" => {
            let b = random_bool_impl().map_err(|e| EvalError::User(format!("random_bool: {e}")))?;
            Value::Bool(
                b,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("random_bool_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "random_float_native" => {
            let f =
                random_float_impl().map_err(|e| EvalError::User(format!("random_float: {e}")))?;
            Value::Float(
                f,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("random_float_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "random_pick_native" => {
            let xs = match args.first() {
                Some(Value::List(items, _)) => items.clone(),
                _ => {
                    return Err(EvalError::TypeError(
                        "random_pick_native(): expected [Str]".into(),
                    ))
                }
            };
            if xs.is_empty() {
                return Err(EvalError::User("random_pick: list is empty".into()));
            }
            let idx = random_int_impl(0, (xs.len() - 1) as i64)
                .map_err(|e| EvalError::User(format!("random_pick: {e}")))?;
            let s = xs[idx as usize]
                .as_str()
                .ok_or_else(|| {
                    EvalError::TypeError("random_pick_native(): list elements must be Str".into())
                })?
                .to_string();
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("random_pick_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::date natives ──────────────────────────────────────────────────
        "date_year_native" => {
            let ms = args
                .first()
                .and_then(Value::as_int)
                .ok_or_else(|| EvalError::TypeError("date_year_native(): expected Int".into()))?;
            let (y, _, _) = ymd_from_ms(ms);
            Value::Int(
                y,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("date_year_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "date_month_native" => {
            let ms = args
                .first()
                .and_then(Value::as_int)
                .ok_or_else(|| EvalError::TypeError("date_month_native(): expected Int".into()))?;
            let (_, m, _) = ymd_from_ms(ms);
            Value::Int(
                m,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("date_month_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "date_day_native" => {
            let ms = args
                .first()
                .and_then(Value::as_int)
                .ok_or_else(|| EvalError::TypeError("date_day_native(): expected Int".into()))?;
            let (_, _, d) = ymd_from_ms(ms);
            Value::Int(
                d,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("date_day_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "date_weekday_native" => {
            let ms = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("date_weekday_native(): expected Int".into())
            })?;
            // 1970-01-01 is day 0; it was a Thursday (4).
            // Use floor division so negative timestamps (pre-1970) work correctly.
            let day_num = if ms >= 0 {
                ms / 86_400_000
            } else {
                (ms - 86_399_999) / 86_400_000
            };
            // day_num + 4 may be negative; use rem_euclid for correct modulo.
            let wd = (day_num + 4).rem_euclid(7); // 0=Sunday
            Value::Int(
                wd,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("date_weekday_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "date_compose_native" => {
            let year = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("date_compose_native(): expected Int year".into())
            })?;
            let month = args.get(1).and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("date_compose_native(): expected Int month".into())
            })?;
            let day = args.get(2).and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("date_compose_native(): expected Int day".into())
            })?;
            let ms = ms_from_ymd(year, month, day);
            Value::Int(
                ms,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("date_compose_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::json natives ──────────────────────────────────────────────────
        "json_parse_value_native" => {
            let s = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("json_parse_value_native(): expected Str".into())
            })?;
            let v: serde_json::Value = serde_json::from_str(s)
                .map_err(|e| EvalError::User(format!("json_parse_value: {e}")))?;
            let canonical = v.to_string();
            Value::Str(
                canonical,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("json_parse_value_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "json_get_str_native" => {
            let json = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("json_get_str_native(): expected Str json".into())
            })?;
            let key = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("json_get_str_native(): expected Str key".into())
            })?;
            let v: serde_json::Value = serde_json::from_str(json)
                .map_err(|e| EvalError::User(format!("json_get_str: parse error: {e}")))?;
            let obj = v
                .as_object()
                .ok_or_else(|| EvalError::User("json_get_str: not a JSON object".into()))?;
            let val = obj
                .get(key)
                .ok_or_else(|| EvalError::User(format!("json_get_str: key {key:?} not found")))?;
            let s = val
                .as_str()
                .ok_or_else(|| {
                    EvalError::User(format!("json_get_str: value for {key:?} is not a string"))
                })?
                .to_string();
            Value::Str(
                s,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("json_get_str_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "json_get_int_native" => {
            let json = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("json_get_int_native(): expected Str json".into())
            })?;
            let key = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("json_get_int_native(): expected Str key".into())
            })?;
            let v: serde_json::Value = serde_json::from_str(json)
                .map_err(|e| EvalError::User(format!("json_get_int: parse error: {e}")))?;
            let obj = v
                .as_object()
                .ok_or_else(|| EvalError::User("json_get_int: not a JSON object".into()))?;
            let val = obj
                .get(key)
                .ok_or_else(|| EvalError::User(format!("json_get_int: key {key:?} not found")))?;
            let n = val.as_i64().ok_or_else(|| {
                EvalError::User(format!("json_get_int: value for {key:?} is not an integer"))
            })?;
            Value::Int(
                n,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("json_get_int_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "json_get_bool_native" => {
            let json = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("json_get_bool_native(): expected Str json".into())
            })?;
            let key = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("json_get_bool_native(): expected Str key".into())
            })?;
            let v: serde_json::Value = serde_json::from_str(json)
                .map_err(|e| EvalError::User(format!("json_get_bool: parse error: {e}")))?;
            let obj = v
                .as_object()
                .ok_or_else(|| EvalError::User("json_get_bool: not a JSON object".into()))?;
            let val = obj
                .get(key)
                .ok_or_else(|| EvalError::User(format!("json_get_bool: key {key:?} not found")))?;
            let b = val.as_bool().ok_or_else(|| {
                EvalError::User(format!("json_get_bool: value for {key:?} is not a boolean"))
            })?;
            Value::Bool(
                b,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("json_get_bool_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "json_keys_native" => {
            let json = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("json_keys_native(): expected Str json".into())
            })?;
            let v: serde_json::Value = serde_json::from_str(json)
                .map_err(|e| EvalError::User(format!("json_keys: parse error: {e}")))?;
            let prov_inner = ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("json_keys_native".into()),
                span,
            );
            let keys: Vec<Value> = match v.as_object() {
                Some(obj) => obj
                    .keys()
                    .map(|k| Value::Str(k.clone(), prov_inner.clone()))
                    .collect(),
                None => vec![],
            };
            Value::List(
                keys,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("json_keys_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::log natives ───────────────────────────────────────────────────
        "log_info_native" => {
            let msg = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("log_info_native(): expected Str".into()))?;
            let ts = format_iso_from_ms(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
            );
            eprintln!("{ts} [INFO] {msg}");
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("log_info_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "log_warn_native" => {
            let msg = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("log_warn_native(): expected Str".into()))?;
            let ts = format_iso_from_ms(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
            );
            eprintln!("{ts} [WARN] {msg}");
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("log_warn_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "log_error_native" => {
            let msg = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("log_error_native(): expected Str".into()))?;
            let ts = format_iso_from_ms(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
            );
            eprintln!("{ts} [ERROR] {msg}");
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("log_error_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "log_debug_native" => {
            let msg = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("log_debug_native(): expected Str".into()))?;
            let level = std::env::var("AETHER_LOG").unwrap_or_default();
            if level.to_lowercase().contains("debug") {
                let ts = format_iso_from_ms(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0),
                );
                eprintln!("{ts} [DEBUG] {msg}");
            }
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("log_debug_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }

        // ── std::term natives ──────────────────────────────────────────────────
        "term_red_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("term_red_native(): expected Str".into()))?;
            let result = term_wrap(s, "\x1b[31m", "\x1b[0m");
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("term_red_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "term_green_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("term_green_native(): expected Str".into()))?;
            let result = term_wrap(s, "\x1b[32m", "\x1b[0m");
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("term_green_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "term_yellow_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("term_yellow_native(): expected Str".into()))?;
            let result = term_wrap(s, "\x1b[33m", "\x1b[0m");
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("term_yellow_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "term_blue_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("term_blue_native(): expected Str".into()))?;
            let result = term_wrap(s, "\x1b[34m", "\x1b[0m");
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("term_blue_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "term_bold_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("term_bold_native(): expected Str".into()))?;
            let result = term_wrap(s, "\x1b[1m", "\x1b[0m");
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("term_bold_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "term_dim_native" => {
            let s = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("term_dim_native(): expected Str".into()))?;
            let result = term_wrap(s, "\x1b[2m", "\x1b[0m");
            Value::Str(
                result,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("term_dim_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::yaml natives ──────────────────────────────────────────────────
        "yaml_get_str_native" => {
            let yaml = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("yaml_get_str_native(): expected Str yaml".into())
            })?;
            let key = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("yaml_get_str_native(): expected Str key".into())
            })?;
            let val = yaml_get_raw(yaml, key)
                .ok_or_else(|| EvalError::User(format!("yaml_get_str: key {key:?} not found")))?;
            Value::Str(
                val,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("yaml_get_str_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "yaml_get_int_native" => {
            let yaml = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("yaml_get_int_native(): expected Str yaml".into())
            })?;
            let key = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("yaml_get_int_native(): expected Str key".into())
            })?;
            let raw = yaml_get_raw(yaml, key)
                .ok_or_else(|| EvalError::User(format!("yaml_get_int: key {key:?} not found")))?;
            let n: i64 = raw.trim().parse().map_err(|_| {
                EvalError::User(format!("yaml_get_int: value {raw:?} is not an integer"))
            })?;
            Value::Int(
                n,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("yaml_get_int_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "yaml_keys_native" => {
            let yaml = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("yaml_keys_native(): expected Str yaml".into())
            })?;
            let prov_inner = ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("yaml_keys_native".into()),
                span,
            );
            let keys: Vec<Value> = yaml_list_keys(yaml)
                .into_iter()
                .map(|k| Value::Str(k, prov_inner.clone()))
                .collect();
            Value::List(
                keys,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("yaml_keys_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::fs natives ───────────────────────────────────────────────────
        "fs_read_native" => {
            let path = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fs_read_native(): expected Str path".into()))?
                .to_string();
            let contents = std::fs::read_to_string(&path)
                .map_err(|e| EvalError::User(format!("fs_read: {path:?}: {e}")))?;
            Value::Str(
                contents,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("fs_read_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "fs_write_native" => {
            let path = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("fs_write_native(): expected Str path".into()))?
                .to_string();
            let contents = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("fs_write_native(): expected Str contents".into())
            })?;
            std::fs::write(&path, contents)
                .map_err(|e| EvalError::User(format!("fs_write: {path:?}: {e}")))?;
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("fs_write_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "fs_append_native" => {
            use std::io::Write as IoWrite;
            let path = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("fs_append_native(): expected Str path".into())
                })?
                .to_string();
            let contents = args.get(1).and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("fs_append_native(): expected Str contents".into())
            })?;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| EvalError::User(format!("fs_append: {path:?}: {e}")))?;
            file.write_all(contents.as_bytes())
                .map_err(|e| EvalError::User(format!("fs_append: {path:?}: {e}")))?;
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("fs_append_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "fs_exists_native" => {
            let path = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("fs_exists_native(): expected Str path".into())
            })?;
            let exists = std::path::Path::new(path).exists();
            Value::Bool(
                exists,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("fs_exists_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "fs_remove_native" => {
            let path = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("fs_remove_native(): expected Str path".into())
                })?
                .to_string();
            std::fs::remove_file(&path)
                .map_err(|e| EvalError::User(format!("fs_remove: {path:?}: {e}")))?;
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("fs_remove_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "fs_list_dir_native" => {
            let path = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("fs_list_dir_native(): expected Str path".into())
                })?
                .to_string();
            let entries = std::fs::read_dir(&path)
                .map_err(|e| EvalError::User(format!("fs_list_dir: {path:?}: {e}")))?;
            let prov_inner = ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("fs_list_dir_native".into()),
                span,
            );
            let mut names: Vec<Value> = Vec::new();
            for entry in entries {
                let entry =
                    entry.map_err(|e| EvalError::User(format!("fs_list_dir: {path:?}: {e}")))?;
                let name = entry.file_name().to_string_lossy().into_owned();
                names.push(Value::Str(name, prov_inner.clone()));
            }
            Value::List(
                names,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("fs_list_dir_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "fs_mkdir_all_native" => {
            let path = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("fs_mkdir_all_native(): expected Str path".into())
                })?
                .to_string();
            std::fs::create_dir_all(&path)
                .map_err(|e| EvalError::User(format!("fs_mkdir_all: {path:?}: {e}")))?;
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("fs_mkdir_all_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }

        // ── std::cache natives ─────────────────────────────────────────────────
        "cache_get_native" => {
            let key = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("cache_get_native(): expected Str key".into())
            })?;
            let val = CACHE_STORE
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .unwrap_or_default();
            Value::Str(
                val,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("cache_get_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "cache_set_native" => {
            let key = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| EvalError::TypeError("cache_set_native(): expected Str key".into()))?
                .to_string();
            let value = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("cache_set_native(): expected Str value".into())
                })?
                .to_string();
            CACHE_STORE
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .insert(key, value);
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("cache_set_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "cache_has_native" => {
            let key = args.first().and_then(Value::as_str).ok_or_else(|| {
                EvalError::TypeError("cache_has_native(): expected Str key".into())
            })?;
            let has = CACHE_STORE
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .contains_key(key);
            Value::Bool(
                has,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("cache_has_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }
        "cache_clear_native" => {
            CACHE_STORE
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .clear();
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("cache_clear_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }

        // ── std::retry natives ─────────────────────────────────────────────────
        "retry_sleep_ms_native" => {
            let ms = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("retry_sleep_ms_native(): expected Int ms".into())
            })?;
            if ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(ms as u64));
            }
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("retry_sleep_ms_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "retry_backoff_ms_native" => {
            let attempt = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("retry_backoff_ms_native(): expected Int attempt".into())
            })?;
            let attempt = attempt.max(0) as u32;
            // 100 * 2^attempt, capped at 30000
            let ms: i64 = if attempt >= 9 {
                30000
            } else {
                (100i64 * (1i64 << attempt)).min(30000)
            };
            Value::Int(
                ms,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("retry_backoff_ms_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        // ── std::http_server natives ───────────────────────────────────────────
        "http_serve_static_native" => {
            let port = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("http_serve_static_native(): expected Int port".into())
            })?;
            let body = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("http_serve_static_native(): expected Str body".into())
                })?
                .to_string();
            http_serve_static_impl(port as u16, body)
                .map_err(|e| EvalError::User(format!("http_serve_static: {e}")))?;
            Value::Unit(ProvChain::extend(
                rt.arena.clone(),
                ProvOp::Call("http_serve_static_native".into()),
                span,
                args.iter().map(|a| a.prov().head).collect(),
            ))
        }
        "http_get_local_native" => {
            let port = args.first().and_then(Value::as_int).ok_or_else(|| {
                EvalError::TypeError("http_get_local_native(): expected Int port".into())
            })?;
            let path = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    EvalError::TypeError("http_get_local_native(): expected Str path".into())
                })?
                .to_string();
            let response = http_get_local_impl(port as u16, &path)
                .map_err(|e| EvalError::User(format!("http_get_local: {e}")))?;
            Value::Str(
                response,
                ProvChain::extend(
                    rt.arena.clone(),
                    ProvOp::Call("http_get_local_native".into()),
                    span,
                    args.iter().map(|a| a.prov().head).collect(),
                ),
            )
        }

        _ => return Ok(None),
    };
    Ok(Some(v))
}

/// Convert a Unix-epoch millisecond timestamp to ISO-8601 `YYYY-MM-DDTHH:MM:SSZ`.
/// Uses pure arithmetic — no external date library required.
fn format_iso_from_ms(ms: i64) -> String {
    // Work in whole seconds; truncate sub-second precision.
    let total_secs = if ms < 0 { 0i64 } else { ms / 1000 };

    // ── Decompose into time-of-day and whole days ──────────────────────────
    let secs_in_day = 86400i64;
    let day_num = total_secs / secs_in_day;
    let rem_secs = total_secs % secs_in_day;

    let hour = rem_secs / 3600;
    let minute = (rem_secs % 3600) / 60;
    let second = rem_secs % 60;

    // ── Decompose day_num (days since 1970-01-01) into year/month/day ──────
    // Algorithm: shift epoch to 1 March 0000 (civil calendar), then use
    // Euclidean algorithm from Howard Hinnant's date.h (public domain).
    let z = day_num + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month of year [0, 11] (Mar=0)
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hour, minute, second
    )
}

// ── std::base64 helpers ────────────────────────────────────────────────────

const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode_impl(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let combined = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_CHARS[((combined >> 18) & 0x3F) as usize] as char);
        out.push(BASE64_CHARS[((combined >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64_CHARS[((combined >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(BASE64_CHARS[(combined & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode_impl(input: &str) -> Result<Vec<u8>, String> {
    // Build reverse lookup: char -> 6-bit value, 255 = invalid
    let mut lut = [255u8; 256];
    for (i, &c) in BASE64_CHARS.iter().enumerate() {
        lut[c as usize] = i as u8;
    }
    // Strip trailing padding
    let input = input.trim_end_matches('=');
    let chars: Vec<u8> = input.bytes().collect();
    // Validate all chars before decoding
    for &c in &chars {
        if lut[c as usize] == 255 {
            return Err(format!("invalid base64 char: {:?}", c as char));
        }
    }
    let n = chars.len();
    // Each group of 4 Base64 chars → 3 bytes; n chars → floor(n*3/4) bytes.
    let out_len = n * 3 / 4;
    let mut buf = Vec::with_capacity(out_len);
    let mut i = 0;
    while i < n {
        let c0 = lut[chars[i] as usize] as u32;
        let c1 = if i + 1 < n {
            lut[chars[i + 1] as usize] as u32
        } else {
            0
        };
        let c2 = if i + 2 < n {
            lut[chars[i + 2] as usize] as u32
        } else {
            0
        };
        let c3 = if i + 3 < n {
            lut[chars[i + 3] as usize] as u32
        } else {
            0
        };
        let combined = (c0 << 18) | (c1 << 12) | (c2 << 6) | c3;
        buf.push(((combined >> 16) & 0xFF) as u8);
        if i + 2 < n {
            buf.push(((combined >> 8) & 0xFF) as u8);
        }
        if i + 3 < n {
            buf.push((combined & 0xFF) as u8);
        }
        i += 4;
    }
    Ok(buf)
}

// ── std::uuid helpers ──────────────────────────────────────────────────────

fn uuid_v4_impl() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
    // Set version 4 (bits 12-15 of byte 6)
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    // Set variant bits 10xx (byte 8 top two bits)
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ))
}

// ── std::random helpers ────────────────────────────────────────────────────

/// Draw 8 random bytes from OS entropy.
fn random_u64() -> Result<u64, String> {
    let mut buf = [0u8; 8];
    getrandom::getrandom(&mut buf).map_err(|e| e.to_string())?;
    Ok(u64::from_le_bytes(buf))
}

fn random_int_impl(lo: i64, hi: i64) -> Result<i64, String> {
    // Rejection sampling to avoid modulo bias.
    let range = (hi - lo) as u64 + 1;
    let threshold = range.wrapping_neg() % range; // = (2^64 - range) % range
    loop {
        let r = random_u64()?;
        if r >= threshold {
            return Ok(lo + (r % range) as i64);
        }
    }
}

fn random_bool_impl() -> Result<bool, String> {
    Ok(random_u64()? & 1 == 1)
}

fn random_float_impl() -> Result<f64, String> {
    // Use top 53 bits for a uniform [0, 1) float.
    let r = random_u64()?;
    Ok((r >> 11) as f64 / (1u64 << 53) as f64)
}

// ── std::date helpers ──────────────────────────────────────────────────────

/// Decompose a Unix-epoch millisecond timestamp into (year, month 1-12, day 1-31).
/// Uses Howard Hinnant's civil calendar algorithm (same as format_iso_from_ms).
/// Supports negative timestamps (pre-1970 dates) via signed floor division.
fn ymd_from_ms(ms: i64) -> (i64, i64, i64) {
    // Floor division for negative ms (Rust % is truncating, not flooring).
    let day_num = if ms >= 0 {
        ms / 86_400_000
    } else {
        (ms - 86_399_999) / 86_400_000
    };
    let z = day_num + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Compose year/month/day into Unix-epoch milliseconds (midnight UTC).
fn ms_from_ymd(year: i64, month: i64, day: i64) -> i64 {
    // Shift so March = month 0 to simplify leap handling.
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let day_num = era * 146097 + doe - 719468;
    day_num * 86_400_000
}

fn call_named(rt: &mut Runtime, name: &str, args: Vec<Value>, span: Span) -> EResult<Value> {
    if let Some(v) = dispatch(rt, name, &args, span)? {
        return Ok(v);
    }
    let f = rt
        .module
        .decls
        .iter()
        .find_map(|d| {
            if let Decl::Fn(f) = d {
                if f.name == name {
                    Some(f.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| EvalError::UndefinedFn(name.into()))?;
    let mut env = crate::env::Env::root();
    rt.eval_fn(&f, args, &mut env)
}

fn build_surface(rt: &Runtime, target: &str) -> ModuleSurface {
    use aether_parser::pretty::{type_, Form};
    let name = if target == "current" {
        rt.module
            .name
            .clone()
            .unwrap_or_else(|| "<anonymous>".into())
    } else {
        target.to_string()
    };
    let mut exports = Vec::new();
    for d in &rt.module.decls {
        match d {
            Decl::Fn(f) => {
                let mut sig = String::from("(");
                for (i, p) in f.params.iter().enumerate() {
                    if i > 0 {
                        sig.push_str(", ");
                    }
                    sig.push_str(&p.name);
                    sig.push_str(": ");
                    sig.push_str(&type_(&p.ty, Form::Verbose));
                }
                sig.push_str(") -> ");
                sig.push_str(&type_(&f.ret, Form::Verbose));
                exports.push(ExportEntry {
                    name: f.name.clone(),
                    kind: "fn",
                    signature: sig,
                    effects: f
                        .effects
                        .effects
                        .iter()
                        .map(|e| e.as_str().to_string())
                        .collect(),
                    doc: f.doc.clone(),
                });
            }
            Decl::Tool(t) => {
                let mut sig = String::from("(");
                for (i, p) in t.params.iter().enumerate() {
                    if i > 0 {
                        sig.push_str(", ");
                    }
                    sig.push_str(&p.name);
                    sig.push_str(": ");
                    sig.push_str(&type_(&p.ty, Form::Verbose));
                }
                sig.push_str(") -> ");
                sig.push_str(&type_(&t.ret, Form::Verbose));
                exports.push(ExportEntry {
                    name: t.name.clone(),
                    kind: "tool",
                    signature: sig,
                    effects: t
                        .effects
                        .effects
                        .iter()
                        .map(|e| e.as_str().to_string())
                        .collect(),
                    doc: t.doc.clone(),
                });
            }
            Decl::Let(l) => {
                let sig =
                    l.ty.as_ref()
                        .map(|t| type_(t, Form::Verbose))
                        .unwrap_or_else(|| "_".into());
                exports.push(ExportEntry {
                    name: l.name.clone(),
                    kind: "let",
                    signature: sig,
                    effects: vec![],
                    doc: l.doc.clone(),
                });
            }
            Decl::TypeAlias(t) => {
                exports.push(ExportEntry {
                    name: t.name.clone(),
                    kind: "type",
                    signature: type_(&t.ty, Form::Verbose),
                    effects: vec![],
                    doc: None,
                });
            }
            _ => {}
        }
    }
    ModuleSurface {
        name,
        doc: rt.module.doc.clone(),
        exports,
    }
}

fn summarize_scope(rt: &Runtime, _scope: &str, budget: usize) -> String {
    let s = build_surface(rt, "current").format();
    if s.len() <= budget {
        s
    } else {
        let mut t = s.chars().take(budget.saturating_sub(1)).collect::<String>();
        t.push('…');
        t
    }
}

fn format_prov(chain: &ProvChain) -> String {
    let nodes = chain.nodes_topo();
    let mut s = String::new();
    s.push_str("provenance:\n");
    for (id, n) in nodes {
        s.push_str(&format!("  #{id}: {:?}", n.op));
        if !n.parents.is_empty() {
            s.push_str(&format!("   <- {:?}", n.parents));
        }
        s.push('\n');
    }
    s
}

// ── std::term helper ──────────────────────────────────────────────────────────

/// Wrap `s` with ANSI `open` / `close` escapes.
/// When `NO_COLOR` is set in the environment, returns `s` unchanged.
fn term_wrap(s: &str, open: &str, close: &str) -> String {
    if std::env::var_os("NO_COLOR").is_some() {
        s.to_string()
    } else {
        format!("{open}{s}{close}")
    }
}

// ── std::yaml helpers ─────────────────────────────────────────────────────────

/// Parse a minimal YAML document and return the raw (unquoted) string value
/// for `key`, or `None` if the key is not present.
///
/// Supported syntax:
///   key: value
///   key: "quoted value"
///   key: 42
///   # comment lines and blank lines are skipped
fn yaml_get_raw(yaml: &str, key: &str) -> Option<String> {
    for line in yaml.lines() {
        // Strip inline comments (but not inside quoted values — keep it simple)
        let line = {
            // Only strip # that appears outside a potential quoted section
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }
            trimmed
        };
        if line.is_empty() {
            continue;
        }
        // Split on first `:` followed by whitespace or end-of-line
        if let Some(colon_pos) = line.find(':') {
            let k = line[..colon_pos].trim();
            if k != key {
                continue;
            }
            let rest = line[colon_pos + 1..].trim();
            // Strip inline # comment from unquoted values
            let rest = if rest.starts_with('"') {
                rest
            } else {
                rest.splitn(2, " #").next().unwrap_or(rest).trim()
            };
            // Unquote if wrapped in double quotes
            let val = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                rest[1..rest.len() - 1].to_string()
            } else {
                rest.to_string()
            };
            return Some(val);
        }
    }
    None
}

/// Return all top-level keys found in a minimal YAML document.
fn yaml_list_keys(yaml: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if let Some(colon_pos) = trimmed.find(':') {
            let k = trimmed[..colon_pos].trim().to_string();
            if !k.is_empty() {
                keys.push(k);
            }
        }
    }
    keys
}

#[cfg(test)]
mod builtin_tests {
    use super::*;
    use crate::Runtime;
    use aether_ast::{Module, Span};

    fn rt() -> Runtime {
        Runtime::new(Module {
            name: None,
            doc: None,
            decls: vec![],
            span: Span::DUMMY,
        })
    }

    fn int_val(rt: &Runtime, n: i64) -> Value {
        Value::Int(
            n,
            ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("test".into()),
                Span::DUMMY,
            ),
        )
    }

    fn str_val(rt: &Runtime, s: &str) -> Value {
        Value::Str(
            s.to_string(),
            ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("test".into()),
                Span::DUMMY,
            ),
        )
    }

    fn list_ints(rt: &Runtime, ns: &[i64]) -> Value {
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let items = ns.iter().map(|&n| int_val(rt, n)).collect();
        Value::List(items, prov)
    }

    fn list_strs(rt: &Runtime, ss: &[&str]) -> Value {
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let items = ss.iter().map(|s| str_val(rt, s)).collect();
        Value::List(items, prov)
    }

    // ── list ──────────────────────────────────────────────────────────────────

    #[test]
    fn list_sum_native_empty() {
        let mut rt = rt();
        let args = vec![list_ints(&rt, &[])];
        let result = dispatch(&mut rt, "list_sum_native", &args, Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    #[test]
    fn list_sum_native_values() {
        let mut rt = rt();
        let args = vec![list_ints(&rt, &[1, 2, 3, 4])];
        let result = dispatch(&mut rt, "list_sum_native", &args, Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(10));
    }

    #[test]
    fn list_max_native_ok() {
        let mut rt = rt();
        let args = vec![list_ints(&rt, &[3, 1, 7, 2])];
        let result = dispatch(&mut rt, "list_max_native", &args, Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(7));
    }

    #[test]
    fn list_max_native_empty_throws() {
        let mut rt = rt();
        let args = vec![list_ints(&rt, &[])];
        let err = dispatch(&mut rt, "list_max_native", &args, Span::DUMMY).unwrap_err();
        assert!(
            err.to_string().contains("empty"),
            "expected 'empty' in error: {err}"
        );
    }

    #[test]
    fn list_contains_native() {
        let mut rt = rt();
        let xs = list_ints(&rt, &[1, 2, 3]);
        let needle2 = int_val(&rt, 2);
        let hit = dispatch(
            &mut rt,
            "list_contains_native",
            &[xs.clone(), needle2],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        assert_eq!(hit.as_bool(), Some(true));
        let needle9 = int_val(&rt, 9);
        let miss = dispatch(&mut rt, "list_contains_native", &[xs, needle9], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(miss.as_bool(), Some(false));
    }

    #[test]
    fn list_count_native() {
        let mut rt = rt();
        let xs = list_ints(&rt, &[1, 2, 2, 3, 2]);
        let needle = int_val(&rt, 2);
        let cnt = dispatch(&mut rt, "list_count_native", &[xs, needle], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(cnt.as_int(), Some(3));
    }

    #[test]
    fn list_reversed_native() {
        let mut rt = rt();
        let xs = list_ints(&rt, &[1, 2, 3]);
        let rev = dispatch(&mut rt, "list_reversed_native", &[xs], Span::DUMMY)
            .unwrap()
            .unwrap();
        if let Value::List(items, _) = rev {
            let ns: Vec<i64> = items.iter().map(|v| v.as_int().unwrap()).collect();
            assert_eq!(ns, vec![3, 2, 1]);
        } else {
            panic!("expected List");
        }
    }

    // ── string ────────────────────────────────────────────────────────────────

    #[test]
    fn str_upper_native() {
        let mut rt = rt();
        let arg = str_val(&rt, "hello World");
        let result = dispatch(&mut rt, "str_upper_native", &[arg], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("HELLO WORLD"));
    }

    #[test]
    fn str_lower_native() {
        let mut rt = rt();
        let arg = str_val(&rt, "Hello WORLD");
        let result = dispatch(&mut rt, "str_lower_native", &[arg], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("hello world"));
    }

    #[test]
    fn str_trim_native() {
        let mut rt = rt();
        let arg = str_val(&rt, "  hi  ");
        let result = dispatch(&mut rt, "str_trim_native", &[arg], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("hi"));
    }

    #[test]
    fn str_contains_native() {
        let mut rt = rt();
        let s = str_val(&rt, "hello world");
        let needle_yes = str_val(&rt, "world");
        let hit = dispatch(
            &mut rt,
            "str_contains_native",
            &[s.clone(), needle_yes],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        assert_eq!(hit.as_bool(), Some(true));
        let needle_no = str_val(&rt, "xyz");
        let miss = dispatch(&mut rt, "str_contains_native", &[s, needle_no], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(miss.as_bool(), Some(false));
    }

    #[test]
    fn str_starts_with_native() {
        let mut rt = rt();
        let s = str_val(&rt, "foobar");
        let pfx_yes = str_val(&rt, "foo");
        let yes = dispatch(
            &mut rt,
            "str_starts_with_native",
            &[s.clone(), pfx_yes],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        assert_eq!(yes.as_bool(), Some(true));
        let pfx_no = str_val(&rt, "bar");
        let no = dispatch(&mut rt, "str_starts_with_native", &[s, pfx_no], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(no.as_bool(), Some(false));
    }

    #[test]
    fn str_split_and_join_round_trip() {
        let mut rt = rt();
        let s = str_val(&rt, "a,b,c");
        let sep = str_val(&rt, ",");
        let sep2 = sep.clone();
        let parts = dispatch(&mut rt, "str_split_on_native", &[s, sep], Span::DUMMY)
            .unwrap()
            .unwrap();
        let joined = dispatch(&mut rt, "str_join_native", &[parts, sep2], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(joined.as_str(), Some("a,b,c"));
    }

    // ── map ───────────────────────────────────────────────────────────────────

    #[test]
    fn map_set_get_round_trip() {
        let mut rt = rt();
        let empty_prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let empty = Value::List(vec![], empty_prov);
        let key_x = str_val(&rt, "x");
        let val_42 = int_val(&rt, 42);
        let m1 = dispatch(
            &mut rt,
            "map_set_native",
            &[empty, key_x.clone(), val_42],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        let val = dispatch(&mut rt, "map_get_native", &[m1, key_x], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(val.as_int(), Some(42));
    }

    #[test]
    fn map_set_overwrite() {
        let mut rt = rt();
        let empty_prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let empty = Value::List(vec![], empty_prov);
        let key_k1 = str_val(&rt, "k");
        let val_1 = int_val(&rt, 1);
        let m1 = dispatch(
            &mut rt,
            "map_set_native",
            &[empty, key_k1, val_1],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        let key_k2 = str_val(&rt, "k");
        let val_99 = int_val(&rt, 99);
        let m2 = dispatch(
            &mut rt,
            "map_set_native",
            &[m1, key_k2.clone(), val_99],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        if let Value::List(items, _) = &m2 {
            assert_eq!(items.len(), 1, "overwrite should not append duplicate");
        }
        let val = dispatch(&mut rt, "map_get_native", &[m2, key_k2], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(val.as_int(), Some(99));
    }

    #[test]
    fn map_get_missing_throws() {
        let mut rt = rt();
        let empty_prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let empty = Value::List(vec![], empty_prov);
        let key = str_val(&rt, "nope");
        let err = dispatch(&mut rt, "map_get_native", &[empty, key], Span::DUMMY).unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected 'not found' in error: {err}"
        );
    }

    #[test]
    fn map_has_native() {
        let mut rt = rt();
        let empty_prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let empty = Value::List(vec![], empty_prov);
        let key_a1 = str_val(&rt, "a");
        let val_1 = int_val(&rt, 1);
        let m = dispatch(
            &mut rt,
            "map_set_native",
            &[empty, key_a1, val_1],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        let key_a2 = str_val(&rt, "a");
        let yes = dispatch(&mut rt, "map_has_native", &[m.clone(), key_a2], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(yes.as_bool(), Some(true));
        let key_b = str_val(&rt, "b");
        let no = dispatch(&mut rt, "map_has_native", &[m, key_b], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(no.as_bool(), Some(false));
    }

    // ── path ──────────────────────────────────────────────────────────────────

    #[test]
    fn path_join_native_basic() {
        let mut rt = rt();
        let a = str_val(&rt, "/foo");
        let b = str_val(&rt, "bar.txt");
        let result = dispatch(&mut rt, "path_join_native", &[a, b], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("/foo/bar.txt"));
    }

    #[test]
    fn path_basename_native_basic() {
        let mut rt = rt();
        let p = str_val(&rt, "/foo/bar.txt");
        let result = dispatch(&mut rt, "path_basename_native", &[p], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("bar.txt"));
    }

    #[test]
    fn path_dirname_native_basic() {
        let mut rt = rt();
        let p = str_val(&rt, "/foo/bar.txt");
        let result = dispatch(&mut rt, "path_dirname_native", &[p], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("/foo"));
    }

    #[test]
    fn path_extension_native_basic() {
        let mut rt = rt();
        let p = str_val(&rt, "archive.tar.gz");
        let result = dispatch(&mut rt, "path_extension_native", &[p], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("gz"));
    }

    #[test]
    fn path_extension_native_no_ext() {
        let mut rt = rt();
        let p = str_val(&rt, "README");
        let result = dispatch(&mut rt, "path_extension_native", &[p], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some(""));
    }

    #[test]
    fn path_exists_native_returns_bool() {
        let mut rt = rt();
        // Use a path that definitely exists on any machine.
        let p = str_val(&rt, "/");
        let result = dispatch(&mut rt, "path_exists_native", &[p], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_bool(), Some(true));
        // Use a path that definitely does not exist.
        let p2 = str_val(&rt, "/this_path_should_not_exist_aether_test_xyz");
        let result2 = dispatch(&mut rt, "path_exists_native", &[p2], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result2.as_bool(), Some(false));
    }

    // ── time ──────────────────────────────────────────────────────────────────

    #[test]
    fn time_now_ms_native_positive() {
        let mut rt = rt();
        let result = dispatch(&mut rt, "time_now_ms_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();
        let ms = result.as_int().unwrap();
        // After year 2020: > 1_580_000_000_000 ms since epoch.
        assert!(
            ms > 1_580_000_000_000,
            "expected positive timestamp, got {ms}"
        );
    }

    #[test]
    fn time_monotonic_ms_native_non_negative() {
        let mut rt = rt();
        let r1 = dispatch(&mut rt, "time_monotonic_ms_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();
        let r2 = dispatch(&mut rt, "time_monotonic_ms_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();
        let t1 = r1.as_int().unwrap();
        let t2 = r2.as_int().unwrap();
        assert!(t1 >= 0, "monotonic time should be non-negative");
        assert!(t2 >= t1, "monotonic time must be non-decreasing");
    }

    #[test]
    fn time_format_iso_native_epoch() {
        let mut rt = rt();
        let zero = int_val(&rt, 0);
        let result = dispatch(&mut rt, "time_format_iso_native", &[zero], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("1970-01-01T00:00:00Z"));
    }

    #[test]
    fn time_format_iso_native_known_date() {
        let mut rt = rt();
        // 2024-01-15T11:30:45Z = 1705318245 seconds = 1705318245000 ms
        let ms = int_val(&rt, 1_705_318_245_000);
        let result = dispatch(&mut rt, "time_format_iso_native", &[ms], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("2024-01-15T11:30:45Z"));
    }

    // ── env ───────────────────────────────────────────────────────────────────

    #[test]
    fn env_set_get_round_trip() {
        let mut rt = rt();
        let key = str_val(&rt, "_AETHER_TEST_VAR_XYZ");
        let val = str_val(&rt, "hello_aether");
        dispatch(&mut rt, "env_set_native", &[key.clone(), val], Span::DUMMY)
            .unwrap()
            .unwrap();
        let got = dispatch(&mut rt, "env_get_native", &[key], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(got.as_str(), Some("hello_aether"));
    }

    #[test]
    fn env_has_native_absent() {
        let mut rt = rt();
        let key = str_val(&rt, "_AETHER_DEFINITELY_NOT_SET_ZZZ");
        let result = dispatch(&mut rt, "env_has_native", &[key], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_bool(), Some(false));
    }

    #[test]
    fn env_get_native_absent_is_empty() {
        let mut rt = rt();
        let key = str_val(&rt, "_AETHER_DEFINITELY_NOT_SET_ZZZ2");
        let result = dispatch(&mut rt, "env_get_native", &[key], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some(""));
    }

    // ── fmt ───────────────────────────────────────────────────────────────────

    #[test]
    fn fmt1_native_replaces_placeholder() {
        let mut rt = rt();
        let tmpl = str_val(&rt, "Hello, {}!");
        let a = str_val(&rt, "world");
        let result = dispatch(&mut rt, "fmt1_native", &[tmpl, a], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("Hello, world!"));
    }

    #[test]
    fn fmt2_native_replaces_two() {
        let mut rt = rt();
        let tmpl = str_val(&rt, "{} + {} = ?");
        let a = str_val(&rt, "1");
        let b = str_val(&rt, "2");
        let result = dispatch(&mut rt, "fmt2_native", &[tmpl, a, b], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("1 + 2 = ?"));
    }

    #[test]
    fn fmt3_native_replaces_three() {
        let mut rt = rt();
        let tmpl = str_val(&rt, "{}-{}-{}");
        let a = str_val(&rt, "a");
        let b = str_val(&rt, "b");
        let c = str_val(&rt, "c");
        let result = dispatch(&mut rt, "fmt3_native", &[tmpl, a, b, c], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("a-b-c"));
    }

    #[test]
    fn fmt1_native_no_placeholder_returns_verbatim() {
        let mut rt = rt();
        let tmpl = str_val(&rt, "no holes here");
        let a = str_val(&rt, "ignored");
        let result = dispatch(&mut rt, "fmt1_native", &[tmpl, a], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("no holes here"));
    }

    #[test]
    fn pad_left_native_pads() {
        let mut rt = rt();
        let s = str_val(&rt, "42");
        let n = int_val(&rt, 5);
        let ch = str_val(&rt, "0");
        let result = dispatch(&mut rt, "pad_left_native", &[s, n, ch], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("00042"));
    }

    #[test]
    fn pad_right_native_pads() {
        let mut rt = rt();
        let s = str_val(&rt, "hi");
        let n = int_val(&rt, 5);
        let ch = str_val(&rt, ".");
        let result = dispatch(&mut rt, "pad_right_native", &[s, n, ch], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("hi..."));
    }

    #[test]
    fn pad_native_no_op_when_long_enough() {
        let mut rt = rt();
        let s = str_val(&rt, "hello");
        let n = int_val(&rt, 3);
        let ch = str_val(&rt, "X");
        let r = dispatch(
            &mut rt,
            "pad_left_native",
            &[s.clone(), n.clone(), ch.clone()],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        assert_eq!(r.as_str(), Some("hello"));
        let r2 = dispatch(&mut rt, "pad_right_native", &[s, n, ch], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(r2.as_str(), Some("hello"));
    }

    // ── result ────────────────────────────────────────────────────────────────

    #[test]
    fn result_throw_native_raises_user_error() {
        let mut rt = rt();
        let msg = str_val(&rt, "something went wrong");
        let err = dispatch(&mut rt, "result_throw_native", &[msg], Span::DUMMY).unwrap_err();
        assert!(
            err.to_string().contains("something went wrong"),
            "expected message in error: {err}"
        );
    }

    // ── format_iso helper ─────────────────────────────────────────────────────

    #[test]
    fn format_iso_epoch() {
        assert_eq!(super::format_iso_from_ms(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_iso_one_day() {
        // 1970-01-02
        assert_eq!(
            super::format_iso_from_ms(86_400_000),
            "1970-01-02T00:00:00Z"
        );
    }

    #[test]
    fn format_iso_negative_clamps_to_epoch() {
        assert_eq!(super::format_iso_from_ms(-1000), "1970-01-01T00:00:00Z");
    }

    // ── std::regex unit tests ─────────────────────────────────────────────────

    #[test]
    fn regex_match_native_basic() {
        let mut rt = rt();
        let pat = str_val(&rt, r"\d+");
        let input = str_val(&rt, "abc 42 def");
        let result = super::dispatch(&mut rt, "regex_match_native", &[pat, input], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_bool(), Some(true));
    }

    #[test]
    fn regex_match_native_no_match() {
        let mut rt = rt();
        let pat = str_val(&rt, r"\d+");
        let input = str_val(&rt, "no digits here");
        let result = super::dispatch(&mut rt, "regex_match_native", &[pat, input], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_bool(), Some(false));
    }

    #[test]
    fn regex_match_native_invalid_pattern() {
        let mut rt = rt();
        let pat = str_val(&rt, r"[invalid");
        let input = str_val(&rt, "anything");
        assert!(
            super::dispatch(&mut rt, "regex_match_native", &[pat, input], Span::DUMMY).is_err()
        );
    }

    #[test]
    fn regex_find_native_basic() {
        let mut rt = rt();
        let pat = str_val(&rt, r"\d+");
        let input = str_val(&rt, "abc 42 def");
        let result = super::dispatch(&mut rt, "regex_find_native", &[pat, input], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("42"));
    }

    #[test]
    fn regex_find_native_no_match_returns_empty() {
        let mut rt = rt();
        let pat = str_val(&rt, r"\d+");
        let input = str_val(&rt, "no digits");
        let result = super::dispatch(&mut rt, "regex_find_native", &[pat, input], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some(""));
    }

    #[test]
    fn regex_replace_native_basic() {
        let mut rt = rt();
        let pat = str_val(&rt, r"\d+");
        let input = str_val(&rt, "foo 42 bar 99");
        let repl = str_val(&rt, "NUM");
        let result = super::dispatch(
            &mut rt,
            "regex_replace_native",
            &[pat, input, repl],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        // replacen(1) — only the first match is replaced
        assert_eq!(result.as_str(), Some("foo NUM bar 99"));
    }

    #[test]
    fn regex_split_native_basic() {
        let mut rt = rt();
        let pat = str_val(&rt, r",\s*");
        let input = str_val(&rt, "a, b, c");
        let result = super::dispatch(&mut rt, "regex_split_native", &[pat, input], Span::DUMMY)
            .unwrap()
            .unwrap();
        if let Value::List(items, _) = result {
            let strs: Vec<&str> = items.iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(strs, vec!["a", "b", "c"]);
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn regex_captures_native_basic() {
        let mut rt = rt();
        let pat = str_val(&rt, r"(\d{4})-(\d{2})-(\d{2})");
        let input = str_val(&rt, "date: 2024-03-15 end");
        let result = super::dispatch(&mut rt, "regex_captures_native", &[pat, input], Span::DUMMY)
            .unwrap()
            .unwrap();
        if let Value::List(items, _) = result {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0].as_str(), Some("2024"));
            assert_eq!(items[1].as_str(), Some("03"));
            assert_eq!(items[2].as_str(), Some("15"));
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn regex_captures_native_no_match_empty_list() {
        let mut rt = rt();
        let pat = str_val(&rt, r"(\d+)");
        let input = str_val(&rt, "no digits");
        let result = super::dispatch(&mut rt, "regex_captures_native", &[pat, input], Span::DUMMY)
            .unwrap()
            .unwrap();
        if let Value::List(items, _) = result {
            assert!(items.is_empty());
        } else {
            panic!("expected List");
        }
    }

    // ── std::sys unit tests ───────────────────────────────────────────────────

    #[test]
    fn sys_args_native_returns_list() {
        let mut rt = rt();
        let result = super::dispatch(&mut rt, "sys_args_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();
        // Just verify it returns a List (args vary per test runner).
        assert!(matches!(result, Value::List(..)));
    }

    #[test]
    fn sys_now_unix_native_positive() {
        let mut rt = rt();
        let result = super::dispatch(&mut rt, "sys_now_unix_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();
        let secs = result.as_int().unwrap();
        // Must be after 2020-01-01 (1577836800).
        assert!(
            secs > 1_577_836_800,
            "sys_now_unix returned implausible value: {secs}"
        );
    }

    #[test]
    fn sys_hostname_native_non_empty() {
        let mut rt = rt();
        let result = super::dispatch(&mut rt, "sys_hostname_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();
        let h = result.as_str().unwrap().to_string();
        assert!(!h.is_empty(), "sys_hostname returned empty string");
    }

    #[test]
    fn sys_spawn_native_echo() {
        let mut rt = rt();
        let cmd = str_val(&rt, "echo");
        let arg = str_val(&rt, "hello");
        let argv = list_strs(&rt, &[]);
        // Build [Str] with one element manually
        let prov = aether_ast::ProvChain::singleton(
            rt.arena.clone(),
            aether_ast::ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let argv_val = Value::List(vec![arg], prov);
        let result = super::dispatch(&mut rt, "sys_spawn_native", &[cmd, argv_val], Span::DUMMY)
            .unwrap()
            .unwrap();
        let out = result.as_str().unwrap().to_string();
        assert!(out.trim() == "hello", "expected 'hello', got: {out:?}");
    }

    // ── std::math unit tests ──────────────────────────────────────────────────

    #[test]
    fn math_pow_native_basic() {
        let mut rt = rt();
        let base = int_val(&rt, 2);
        let exp = int_val(&rt, 10);
        let result = super::dispatch(&mut rt, "math_pow_native", &[base, exp], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(1024));
    }

    #[test]
    fn math_pow_native_zero_exp() {
        let mut rt = rt();
        let base = int_val(&rt, 99);
        let exp = int_val(&rt, 0);
        let result = super::dispatch(&mut rt, "math_pow_native", &[base, exp], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(1));
    }

    fn float_val(rt: &Runtime, f: f64) -> Value {
        Value::Float(
            f,
            aether_ast::ProvChain::singleton(
                rt.arena.clone(),
                aether_ast::ProvOp::Synthetic("test".into()),
                Span::DUMMY,
            ),
        )
    }

    #[test]
    fn math_sqrt_native_basic() {
        let mut rt = rt();
        let x = float_val(&rt, 9.0);
        let result = super::dispatch(&mut rt, "math_sqrt_native", &[x], Span::DUMMY)
            .unwrap()
            .unwrap();
        let f = result.as_float().unwrap();
        assert!((f - 3.0).abs() < 1e-9, "sqrt(9) should be 3.0, got {f}");
    }

    #[test]
    fn math_sqrt_native_negative_returns_zero() {
        let mut rt = rt();
        let x = float_val(&rt, -4.0);
        let result = super::dispatch(&mut rt, "math_sqrt_native", &[x], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_float(), Some(0.0));
    }

    #[test]
    fn math_floor_native_basic() {
        let mut rt = rt();
        let x = float_val(&rt, 3.7);
        let result = super::dispatch(&mut rt, "math_floor_native", &[x], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(3));
    }

    #[test]
    fn math_ceil_native_basic() {
        let mut rt = rt();
        let x = float_val(&rt, 3.1);
        let result = super::dispatch(&mut rt, "math_ceil_native", &[x], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(4));
    }

    #[test]
    fn math_round_native_half_up() {
        let mut rt = rt();
        let x = float_val(&rt, 2.5);
        let result = super::dispatch(&mut rt, "math_round_native", &[x], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(3));
    }

    #[test]
    fn math_min_f_native_basic() {
        let mut rt = rt();
        let a = float_val(&rt, 1.5);
        let b = float_val(&rt, 2.5);
        let result = super::dispatch(&mut rt, "math_min_f_native", &[a, b], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_float(), Some(1.5));
    }

    #[test]
    fn math_max_f_native_basic() {
        let mut rt = rt();
        let a = float_val(&rt, 1.5);
        let b = float_val(&rt, 2.5);
        let result = super::dispatch(&mut rt, "math_max_f_native", &[a, b], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_float(), Some(2.5));
    }

    #[test]
    fn math_pi_native_value() {
        let mut rt = rt();
        let result = super::dispatch(&mut rt, "math_pi_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();
        let pi = result.as_float().unwrap();
        assert!((pi - std::f64::consts::PI).abs() < 1e-12);
    }

    // ── std::base64 unit tests ────────────────────────────────────────────────

    #[test]
    fn base64_encode_decode_roundtrip_ascii() {
        let cases = ["", "hello", "Man", "any carnal pleasure.", "hello world!"];
        for s in &cases {
            let enc = super::base64_encode_impl(s.as_bytes());
            let dec = super::base64_decode_impl(&enc).unwrap();
            assert_eq!(dec, s.as_bytes(), "round-trip failed for: {s:?}");
        }
    }

    #[test]
    fn base64_encode_known_vectors() {
        // RFC 4648 §10 test vectors
        assert_eq!(super::base64_encode_impl(b""), "");
        assert_eq!(super::base64_encode_impl(b"f"), "Zg==");
        assert_eq!(super::base64_encode_impl(b"fo"), "Zm8=");
        assert_eq!(super::base64_encode_impl(b"foo"), "Zm9v");
        assert_eq!(super::base64_encode_impl(b"foob"), "Zm9vYg==");
        assert_eq!(super::base64_encode_impl(b"fooba"), "Zm9vYmE=");
        assert_eq!(super::base64_encode_impl(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_roundtrip_binary_bytes() {
        // Edge case: all byte values 0–255
        let binary: Vec<u8> = (0u8..=255u8).collect();
        let enc = super::base64_encode_impl(&binary);
        let dec = super::base64_decode_impl(&enc).unwrap();
        assert_eq!(dec, binary);
    }

    #[test]
    fn base64_decode_invalid_char_errors() {
        let err = super::base64_decode_impl("Z!==");
        assert!(err.is_err(), "expected error for invalid char");
    }

    #[test]
    fn base64_encode_native_dispatch() {
        let mut rt = rt();
        let input = str_val(&rt, "hello");
        let result = super::dispatch(&mut rt, "base64_encode_native", &[input], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("aGVsbG8="));
    }

    #[test]
    fn base64_decode_native_dispatch() {
        let mut rt = rt();
        let input = str_val(&rt, "aGVsbG8=");
        let result = super::dispatch(&mut rt, "base64_decode_native", &[input], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("hello"));
    }

    // ── std::hash unit tests ──────────────────────────────────────────────────

    #[test]
    fn hash_sha256_known_vector() {
        let mut rt = rt();
        let s = str_val(&rt, "hello");
        let result = super::dispatch(&mut rt, "hash_sha256_native", &[s], Span::DUMMY)
            .unwrap()
            .unwrap();
        // NIST/known SHA-256 of "hello"
        assert_eq!(
            result.as_str(),
            Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
    }

    #[test]
    fn hash_sha256_empty_string() {
        let mut rt = rt();
        let s = str_val(&rt, "");
        let result = super::dispatch(&mut rt, "hash_sha256_native", &[s], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(
            result.as_str(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }

    #[test]
    fn hash_default_deterministic() {
        let mut rt = rt();
        let a = str_val(&rt, "cache-key");
        let b = str_val(&rt, "cache-key");
        let r1 = super::dispatch(&mut rt, "hash_default_native", &[a], Span::DUMMY)
            .unwrap()
            .unwrap()
            .as_int()
            .unwrap();
        let r2 = super::dispatch(&mut rt, "hash_default_native", &[b], Span::DUMMY)
            .unwrap()
            .unwrap()
            .as_int()
            .unwrap();
        assert_eq!(
            r1, r2,
            "hash_default must be deterministic within a process"
        );
    }

    #[test]
    fn hash_default_different_inputs_differ() {
        let mut rt = rt();
        let a = str_val(&rt, "foo");
        let b = str_val(&rt, "bar");
        let r1 = super::dispatch(&mut rt, "hash_default_native", &[a], Span::DUMMY)
            .unwrap()
            .unwrap()
            .as_int()
            .unwrap();
        let r2 = super::dispatch(&mut rt, "hash_default_native", &[b], Span::DUMMY)
            .unwrap()
            .unwrap()
            .as_int()
            .unwrap();
        assert_ne!(
            r1, r2,
            "hash_default(\"foo\") should differ from hash_default(\"bar\")"
        );
    }

    // ── std::uuid unit tests ──────────────────────────────────────────────────

    #[test]
    fn uuid_v4_format() {
        let u = super::uuid_v4_impl().unwrap();
        // xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx (36 chars)
        assert_eq!(u.len(), 36, "UUID v4 must be 36 chars: {u}");
        let parts: Vec<&str> = u.split('-').collect();
        assert_eq!(
            parts.len(),
            5,
            "UUID must have 5 hyphen-separated groups: {u}"
        );
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        // version nibble must be '4'
        assert_eq!(&u[14..15], "4", "version nibble must be 4: {u}");
        // variant bits: first hex char of group 4 must be 8, 9, a, or b
        let variant_char = u.chars().nth(19).unwrap();
        assert!(
            matches!(variant_char, '8' | '9' | 'a' | 'b'),
            "variant char must be 8/9/a/b, got {variant_char}: {u}"
        );
    }

    #[test]
    fn uuid_v4_native_dispatch() {
        let mut rt = rt();
        let result = super::dispatch(&mut rt, "uuid_v4_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();
        let s = result.as_str().unwrap().to_string();
        assert_eq!(s.len(), 36);
        assert_eq!(&s[14..15], "4");
    }

    #[test]
    fn uuid_short_is_8_hex_chars() {
        let mut rt = rt();
        let result = super::dispatch(&mut rt, "uuid_short_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();
        let s = result.as_str().unwrap().to_string();
        assert_eq!(s.len(), 8, "uuid_short must be 8 chars: {s}");
        assert!(
            s.chars().all(|c| c.is_ascii_hexdigit()),
            "uuid_short must be hex: {s}"
        );
    }

    // ── std::random unit tests ────────────────────────────────────────────────

    #[test]
    fn random_int_degenerate_range() {
        // lo == hi must always return lo
        let n = super::random_int_impl(42, 42).unwrap();
        assert_eq!(n, 42);
    }

    #[test]
    fn random_int_range_bounds() {
        for _ in 0..50 {
            let n = super::random_int_impl(1, 10).unwrap();
            assert!((1..=10).contains(&n), "random_int out of [1,10]: {n}");
        }
    }

    #[test]
    fn random_int_hi_lt_lo_errors() {
        let mut rt = rt();
        let lo = int_val(&rt, 5);
        let hi = int_val(&rt, 3);
        let err = super::dispatch(&mut rt, "random_int_native", &[lo, hi], Span::DUMMY);
        assert!(err.is_err(), "expected error when hi < lo");
    }

    #[test]
    fn random_bool_returns_bool() {
        let b = super::random_bool_impl().unwrap();
        assert!(b || !b); // tautology — just confirm no panic
    }

    #[test]
    fn random_float_in_unit_interval() {
        for _ in 0..20 {
            let f = super::random_float_impl().unwrap();
            assert!((0.0..1.0).contains(&f), "random_float out of [0,1): {f}");
        }
    }

    #[test]
    fn random_pick_single_element() {
        let mut rt = rt();
        let prov = aether_ast::ProvChain::singleton(
            rt.arena.clone(),
            aether_ast::ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let s = Value::Str("only".into(), prov.clone());
        let xs = Value::List(vec![s], prov);
        let result = super::dispatch(&mut rt, "random_pick_native", &[xs], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("only"));
    }

    #[test]
    fn random_pick_empty_list_errors() {
        let mut rt = rt();
        let prov = aether_ast::ProvChain::singleton(
            rt.arena.clone(),
            aether_ast::ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let xs = Value::List(vec![], prov);
        let err = super::dispatch(&mut rt, "random_pick_native", &[xs], Span::DUMMY);
        assert!(err.is_err(), "expected error for empty list");
    }

    // ── std::date unit tests ──────────────────────────────────────────────────

    #[test]
    fn date_year_epoch() {
        assert_eq!(super::ymd_from_ms(0), (1970, 1, 1));
    }

    #[test]
    fn date_compose_epoch() {
        assert_eq!(super::ms_from_ymd(1970, 1, 1), 0);
    }

    #[test]
    fn date_roundtrip_known_dates() {
        let cases = [
            (2024, 3, 15),
            (2000, 2, 29), // leap year
            (1900, 3, 1),  // not a leap year
            (2100, 3, 1),  // not a leap year (div 100 but not 400)
            (2000, 1, 1),  // Y2K
        ];
        for (y, m, d) in cases {
            let ms = super::ms_from_ymd(y, m, d);
            let (ry, rm, rd) = super::ymd_from_ms(ms);
            assert_eq!(
                (ry, rm, rd),
                (y, m, d),
                "round-trip failed for {y}-{m:02}-{d:02}"
            );
        }
    }

    #[test]
    fn date_weekday_epoch_is_thursday() {
        let mut rt = rt();
        let ms = int_val(&rt, 0);
        let result = super::dispatch(&mut rt, "date_weekday_native", &[ms], Span::DUMMY)
            .unwrap()
            .unwrap();
        // 1970-01-01 was Thursday = 4
        assert_eq!(result.as_int(), Some(4));
    }

    #[test]
    fn date_weekday_known_sunday() {
        let mut rt = rt();
        // 2024-03-10 was a Sunday. ms = days_since_epoch * 86400000
        // days from 1970-01-01 to 2024-03-10 = 19792
        let ms_val = int_val(&rt, 19792i64 * 86_400_000);
        let result = super::dispatch(&mut rt, "date_weekday_native", &[ms_val], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(0), "2024-03-10 should be Sunday (0)");
    }

    #[test]
    fn date_year_native_dispatch() {
        let mut rt = rt();
        let ms = int_val(&rt, 0);
        let result = super::dispatch(&mut rt, "date_year_native", &[ms], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(1970));
    }

    #[test]
    fn date_compose_native_dispatch() {
        let mut rt = rt();
        let y = int_val(&rt, 1970);
        let m = int_val(&rt, 1);
        let d = int_val(&rt, 1);
        let result = super::dispatch(&mut rt, "date_compose_native", &[y, m, d], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    // ── std::json unit tests ───────────────────────────────────────────────────

    #[test]
    fn json_parse_value_roundtrip() {
        let mut rt = rt();
        let input = r#"{"name":"Alice","age":30,"active":true}"#;
        let arg = str_val(&rt, input);
        let result = super::dispatch(&mut rt, "json_parse_value_native", &[arg], Span::DUMMY)
            .unwrap()
            .unwrap();
        // serde_json canonicalises — re-parse and check field present
        let s = result.as_str().unwrap().to_string();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["name"], "Alice");
        assert_eq!(v["age"], 30);
    }

    #[test]
    fn json_get_str_extracts_field() {
        let mut rt = rt();
        let json = str_val(&rt, r#"{"city":"Paris","country":"France"}"#);
        let key = str_val(&rt, "city");
        let result = super::dispatch(&mut rt, "json_get_str_native", &[json, key], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("Paris"));
    }

    #[test]
    fn json_get_int_extracts_field() {
        let mut rt = rt();
        let json = str_val(&rt, r#"{"x":42,"y":-7}"#);
        let key = str_val(&rt, "x");
        let result = super::dispatch(&mut rt, "json_get_int_native", &[json, key], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(42));
    }

    #[test]
    fn json_get_bool_extracts_field() {
        let mut rt = rt();
        let json = str_val(&rt, r#"{"ok":true,"fail":false}"#);
        let key = str_val(&rt, "ok");
        let result = super::dispatch(&mut rt, "json_get_bool_native", &[json, key], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_bool(), Some(true));
    }

    #[test]
    fn json_keys_returns_top_level_keys() {
        let mut rt = rt();
        let json = str_val(&rt, r#"{"a":1,"b":2,"c":3}"#);
        let result = super::dispatch(&mut rt, "json_keys_native", &[json], Span::DUMMY)
            .unwrap()
            .unwrap();
        if let Value::List(items, _) = result {
            let mut keys: Vec<&str> = items.iter().map(|v| v.as_str().unwrap()).collect();
            keys.sort_unstable();
            assert_eq!(keys, vec!["a", "b", "c"]);
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn json_parse_value_rejects_malformed() {
        let mut rt = rt();
        let arg = str_val(&rt, "{not valid json}");
        let result = super::dispatch(&mut rt, "json_parse_value_native", &[arg], Span::DUMMY);
        assert!(result.is_err(), "expected parse error on malformed JSON");
    }

    // ── std::term unit tests ───────────────────────────────────────────────────

    /// Tests that mutate `NO_COLOR` need to be serialized — otherwise
    /// `cargo test` runs them in parallel threads and the env var leaks.
    fn term_env_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn term_red_wraps_with_ansi() {
        let _g = term_env_lock();
        std::env::remove_var("NO_COLOR");
        let s = super::term_wrap("hello", "\x1b[31m", "\x1b[0m");
        assert!(s.starts_with("\x1b[31m"), "expected red open escape");
        assert!(s.ends_with("\x1b[0m"), "expected reset close escape");
        assert!(s.contains("hello"));
    }

    #[test]
    fn term_no_color_honored() {
        let _g = term_env_lock();
        std::env::set_var("NO_COLOR", "1");
        let s = super::term_wrap("hello", "\x1b[31m", "\x1b[0m");
        std::env::remove_var("NO_COLOR");
        assert_eq!(s, "hello", "NO_COLOR should suppress ANSI escapes");
    }

    // ── std::log unit tests ────────────────────────────────────────────────────

    #[test]
    fn log_debug_suppressed_without_env() {
        std::env::remove_var("AETHER_LOG");
        let mut rt = rt();
        let msg = str_val(&rt, "should not appear");
        // Should succeed (not error) even when suppressed
        let result = super::dispatch(&mut rt, "log_debug_native", &[msg], Span::DUMMY);
        assert!(
            result.is_ok(),
            "log_debug_native should not error when suppressed"
        );
    }

    #[test]
    fn log_debug_enabled_with_env() {
        std::env::set_var("AETHER_LOG", "debug");
        let mut rt = rt();
        let msg = str_val(&rt, "debug message");
        let result = super::dispatch(&mut rt, "log_debug_native", &[msg], Span::DUMMY);
        std::env::remove_var("AETHER_LOG");
        assert!(
            result.is_ok(),
            "log_debug_native should succeed when debug enabled"
        );
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn log_info_writes_to_unit() {
        let mut rt = rt();
        let msg = str_val(&rt, "info test");
        let result = super::dispatch(&mut rt, "log_info_native", &[msg], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert!(matches!(result, Value::Unit(_)));
    }

    // ── std::yaml unit tests ───────────────────────────────────────────────────

    #[test]
    fn yaml_get_raw_basic_key_value() {
        let yaml = "name: Alice\nage: 30\n";
        assert_eq!(super::yaml_get_raw(yaml, "name"), Some("Alice".to_string()));
        assert_eq!(super::yaml_get_raw(yaml, "age"), Some("30".to_string()));
    }

    #[test]
    fn yaml_get_raw_quoted_value() {
        let yaml = r#"title: "Hello, World!""#;
        assert_eq!(
            super::yaml_get_raw(yaml, "title"),
            Some("Hello, World!".to_string())
        );
    }

    #[test]
    fn yaml_get_raw_skips_comments() {
        let yaml = "# top comment\nkey: value\n# another comment\n";
        assert_eq!(super::yaml_get_raw(yaml, "key"), Some("value".to_string()));
        assert_eq!(super::yaml_get_raw(yaml, "# top comment"), None);
    }

    #[test]
    fn yaml_get_raw_missing_key_returns_none() {
        let yaml = "a: 1\nb: 2\n";
        assert_eq!(super::yaml_get_raw(yaml, "c"), None);
    }

    #[test]
    fn yaml_list_keys_basic() {
        let yaml = "name: Alice\nage: 30\ncity: Paris\n";
        let keys = super::yaml_list_keys(yaml);
        assert_eq!(keys, vec!["name", "age", "city"]);
    }

    #[test]
    fn yaml_list_keys_skips_comments_and_blanks() {
        let yaml = "# comment\n\nfoo: 1\nbar: 2\n";
        let keys = super::yaml_list_keys(yaml);
        assert_eq!(keys, vec!["foo", "bar"]);
    }

    #[test]
    fn yaml_get_int_native_dispatch() {
        let mut rt = rt();
        let yaml = str_val(&rt, "port: 8080\nworkers: 4\n");
        let key = str_val(&rt, "port");
        let result = super::dispatch(&mut rt, "yaml_get_int_native", &[yaml, key], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_int(), Some(8080));
    }
}

// ── http_server helpers ────────────────────────────────────────────────────

fn http_serve_static_impl(port: u16, body: String) -> Result<(), String> {
    use std::io::{BufRead, BufReader, Write as IoWrite};
    use std::net::TcpListener;

    // AETHER_SERVE_LIMIT: number of requests to handle before returning.
    // 0 means infinite (suitable for production demos).
    // Default is 1 so CI tests don't hang.
    let limit: usize = std::env::var("AETHER_SERVE_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).map_err(|e| format!("bind {addr}: {e}"))?;

    let mut handled = 0usize;
    loop {
        if limit != 0 && handled >= limit {
            break;
        }
        let (mut stream, _peer) = match listener.accept() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("http_serve_static: accept error: {e}");
                continue;
            }
        };

        // Read request line + headers (discard them; we serve everything).
        {
            let reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
            for line in reader.lines() {
                match line {
                    Ok(l) if l.trim().is_empty() => break, // end of headers
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }

        let response = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        // Best-effort write; ignore errors on individual connections.
        let _ = stream.write_all(response.as_bytes());
        handled += 1;
    }
    Ok(())
}

fn http_get_local_impl(port: u16, path: &str) -> Result<String, String> {
    use std::io::{BufRead, BufReader, Write as IoWrite};
    use std::net::TcpStream;

    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;

    // Send a minimal HTTP/1.0 GET so no keep-alive handling is needed.
    let request = format!(
        "GET {} HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        path
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write request: {e}"))?;

    // Read all bytes then parse: skip headers, return body.
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    // Consume status line.
    let status_line = lines
        .next()
        .ok_or_else(|| "empty response".to_string())?
        .map_err(|e| format!("read status: {e}"))?;

    if !status_line.starts_with("HTTP/") {
        return Err(format!("unexpected status line: {status_line:?}"));
    }

    // Consume headers until blank line.
    for line in lines.by_ref() {
        match line {
            Ok(l) if l.trim().is_empty() => break,
            Ok(_) => {}
            Err(e) => return Err(format!("read headers: {e}")),
        }
    }

    // Collect body lines.
    let mut body = String::new();
    for line in lines {
        match line {
            Ok(l) => {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(&l);
            }
            Err(e) => return Err(format!("read body: {e}")),
        }
    }
    Ok(body)
}

#[cfg(test)]
mod new_builtin_tests {
    use super::*;
    use crate::Runtime;
    use aether_ast::{Module, Span};

    fn rt() -> Runtime {
        Runtime::new(Module {
            name: None,
            doc: None,
            decls: vec![],
            span: Span::DUMMY,
        })
    }

    fn int_val(rt: &Runtime, n: i64) -> Value {
        Value::Int(
            n,
            ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("test".into()),
                Span::DUMMY,
            ),
        )
    }

    fn str_val(rt: &Runtime, s: &str) -> Value {
        Value::Str(
            s.to_string(),
            ProvChain::singleton(
                rt.arena.clone(),
                ProvOp::Synthetic("test".into()),
                Span::DUMMY,
            ),
        )
    }

    // ── std::fs unit tests ─────────────────────────────────────────────────

    #[test]
    fn fs_write_and_read_roundtrip() {
        let mut rt = rt();
        let path = std::env::temp_dir().join(format!("aether_fs_unit_{}.txt", std::process::id()));
        let path_str = path.to_string_lossy().to_string();
        let p = str_val(&rt, &path_str);
        let contents = str_val(&rt, "roundtrip content");
        dispatch(
            &mut rt,
            "fs_write_native",
            &[p.clone(), contents],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        let result = dispatch(&mut rt, "fs_read_native", &[p], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("roundtrip content"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fs_append_accumulates() {
        let mut rt = rt();
        let path =
            std::env::temp_dir().join(format!("aether_fs_append_{}.txt", std::process::id()));
        let path_str = path.to_string_lossy().to_string();
        let p = str_val(&rt, &path_str);
        let _ = std::fs::remove_file(&path);
        let va = str_val(&rt, "a");
        let vb = str_val(&rt, "b");
        dispatch(&mut rt, "fs_append_native", &[p.clone(), va], Span::DUMMY).unwrap();
        dispatch(&mut rt, "fs_append_native", &[p.clone(), vb], Span::DUMMY).unwrap();
        let result = dispatch(&mut rt, "fs_read_native", &[p], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("ab"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fs_exists_true_and_false() {
        let mut rt = rt();
        let path =
            std::env::temp_dir().join(format!("aether_fs_exists_{}.txt", std::process::id()));
        let path_str = path.to_string_lossy().to_string();
        let p = str_val(&rt, &path_str);
        let _ = std::fs::remove_file(&path);
        let before = dispatch(&mut rt, "fs_exists_native", &[p.clone()], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(before.as_bool(), Some(false));
        std::fs::write(&path, "x").unwrap();
        let after = dispatch(&mut rt, "fs_exists_native", &[p], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(after.as_bool(), Some(true));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fs_remove_missing_errors() {
        let mut rt = rt();
        let path_str = "/tmp/aether_unit_definitely_missing_xyzzy.txt".to_string();
        let _ = std::fs::remove_file(&path_str);
        let p = str_val(&rt, &path_str);
        let result = dispatch(&mut rt, "fs_remove_native", &[p], Span::DUMMY);
        assert!(result.is_err(), "expected error removing nonexistent file");
    }

    #[test]
    fn fs_list_dir_returns_names() {
        let mut rt = rt();
        let dir = std::env::temp_dir().join(format!("aether_fs_list_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "").unwrap();
        std::fs::write(dir.join("b.txt"), "").unwrap();
        let dir_str = dir.to_string_lossy().to_string();
        let d = str_val(&rt, &dir_str);
        let result = dispatch(&mut rt, "fs_list_dir_native", &[d], Span::DUMMY)
            .unwrap()
            .unwrap();
        if let Value::List(items, _) = result {
            let mut names: Vec<&str> = items.iter().map(|v| v.as_str().unwrap()).collect();
            names.sort_unstable();
            assert_eq!(names, vec!["a.txt", "b.txt"]);
        } else {
            panic!("expected List");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fs_mkdir_all_creates_nested() {
        let mut rt = rt();
        let base = std::env::temp_dir().join(format!("aether_mkdir_{}", std::process::id()));
        let dir = base.join("a").join("b");
        let _ = std::fs::remove_dir_all(&base);
        let dir_str = dir.to_string_lossy().to_string();
        let d = str_val(&rt, &dir_str);
        dispatch(&mut rt, "fs_mkdir_all_native", &[d], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert!(dir.is_dir(), "nested dirs should exist");
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── std::cache unit tests ──────────────────────────────────────────────

    #[test]
    fn cache_set_get_has_clear() {
        let mut rt = rt();
        let key = format!("unit_test_key_{}", std::process::id());
        let k = str_val(&rt, &key);
        let v = str_val(&rt, "val1");

        let before = dispatch(&mut rt, "cache_has_native", &[k.clone()], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(before.as_bool(), Some(false));

        dispatch(&mut rt, "cache_set_native", &[k.clone(), v], Span::DUMMY)
            .unwrap()
            .unwrap();

        let after_has = dispatch(&mut rt, "cache_has_native", &[k.clone()], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(after_has.as_bool(), Some(true));

        let got = dispatch(&mut rt, "cache_get_native", &[k.clone()], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(got.as_str(), Some("val1"));

        dispatch(&mut rt, "cache_clear_native", &[], Span::DUMMY)
            .unwrap()
            .unwrap();

        let after_clear = dispatch(&mut rt, "cache_has_native", &[k.clone()], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(after_clear.as_bool(), Some(false));

        let empty = dispatch(&mut rt, "cache_get_native", &[k], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(empty.as_str(), Some(""));
    }

    // ── std::retry unit tests ──────────────────────────────────────────────

    #[test]
    fn retry_backoff_exponential() {
        let mut rt = rt();
        let cases: &[(i64, i64)] = &[
            (0, 100),
            (1, 200),
            (2, 400),
            (3, 800),
            (4, 1600),
            (8, 25600),
            (9, 30000),
            (20, 30000),
        ];
        for &(attempt, expected) in cases {
            let a = int_val(&rt, attempt);
            let result = dispatch(&mut rt, "retry_backoff_ms_native", &[a], Span::DUMMY)
                .unwrap()
                .unwrap();
            assert_eq!(
                result.as_int(),
                Some(expected),
                "backoff({attempt}) should be {expected}"
            );
        }
    }

    #[test]
    fn retry_sleep_zero_is_noop() {
        let mut rt = rt();
        let ms = int_val(&rt, 0);
        let result = dispatch(&mut rt, "retry_sleep_ms_native", &[ms], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert!(matches!(result, Value::Unit(_)));
    }

    // ── std::http_server unit tests ────────────────────────────────────────

    #[test]
    fn http_serve_and_get_local() {
        let port: u16 = 5741;
        let body = "unit test body".to_string();
        let body2 = body.clone();

        std::env::set_var("AETHER_SERVE_LIMIT", "1");
        let handle = std::thread::spawn(move || {
            http_serve_static_impl(port, body2).unwrap();
        });

        std::thread::sleep(std::time::Duration::from_millis(50));

        let resp = http_get_local_impl(port, "/").unwrap();
        assert_eq!(resp, body, "response body should match");
        handle.join().unwrap();
        std::env::remove_var("AETHER_SERVE_LIMIT");
    }

    #[test]
    fn http_get_local_refused_errors() {
        let result = http_get_local_impl(5742, "/");
        assert!(result.is_err(), "expected connection refused error");
    }

    // ── regex_replace_all unit tests ───────────────────────────────────────

    #[test]
    fn regex_replace_all_replaces_globally() {
        let mut rt = rt();
        let pat = str_val(&rt, "a");
        let input = str_val(&rt, "banana");
        let repl = str_val(&rt, "X");
        let result = dispatch(
            &mut rt,
            "regex_replace_all_native",
            &[pat, input, repl],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.as_str(), Some("bXnXnX"));
    }

    #[test]
    fn regex_replace_all_no_match_unchanged() {
        let mut rt = rt();
        let pat = str_val(&rt, "z");
        let input = str_val(&rt, "banana");
        let repl = str_val(&rt, "X");
        let result = dispatch(
            &mut rt,
            "regex_replace_all_native",
            &[pat, input, repl],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.as_str(), Some("banana"));
    }

    // ── fmt_list unit tests ────────────────────────────────────────────────

    #[test]
    fn fmt_list_replaces_all_placeholders() {
        let mut rt = rt();
        let tmpl = str_val(&rt, "{} {} {}");
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let args = Value::List(
            vec![str_val(&rt, "a"), str_val(&rt, "b"), str_val(&rt, "c")],
            prov,
        );
        let result = dispatch(&mut rt, "fmt_list_native", &[tmpl, args], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("a b c"));
    }

    #[test]
    fn fmt_list_fewer_args_leaves_extra_placeholders() {
        let mut rt = rt();
        let tmpl = str_val(&rt, "{} {} {}");
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let args = Value::List(vec![str_val(&rt, "x")], prov);
        let result = dispatch(&mut rt, "fmt_list_native", &[tmpl, args], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("x {} {}"));
    }

    #[test]
    fn fmt_list_more_args_than_placeholders_ignores_extras() {
        let mut rt = rt();
        let tmpl = str_val(&rt, "{}");
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let args = Value::List(
            vec![str_val(&rt, "a"), str_val(&rt, "b"), str_val(&rt, "c")],
            prov,
        );
        let result = dispatch(&mut rt, "fmt_list_native", &[tmpl, args], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("a"));
    }

    // ── strlist unit tests ─────────────────────────────────────────────────

    #[test]
    fn strlist_head_returns_first() {
        let mut rt = rt();
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let xs = Value::List(vec![str_val(&rt, "foo"), str_val(&rt, "bar")], prov);
        let result = dispatch(&mut rt, "strlist_head_native", &[xs], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("foo"));
    }

    #[test]
    fn strlist_head_empty_throws() {
        let mut rt = rt();
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let xs = Value::List(vec![], prov);
        let err = dispatch(&mut rt, "strlist_head_native", &[xs], Span::DUMMY).unwrap_err();
        assert!(
            err.to_string().contains("empty"),
            "expected empty-list error, got: {err}"
        );
    }

    #[test]
    fn strlist_tail_returns_rest() {
        let mut rt = rt();
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let xs = Value::List(
            vec![str_val(&rt, "a"), str_val(&rt, "b"), str_val(&rt, "c")],
            prov,
        );
        let result = dispatch(&mut rt, "strlist_tail_native", &[xs], Span::DUMMY)
            .unwrap()
            .unwrap();
        if let Value::List(items, _) = result {
            let strs: Vec<&str> = items.iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(strs, vec!["b", "c"]);
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn strlist_tail_empty_returns_empty() {
        let mut rt = rt();
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let xs = Value::List(vec![], prov);
        let result = dispatch(&mut rt, "strlist_tail_native", &[xs], Span::DUMMY)
            .unwrap()
            .unwrap();
        if let Value::List(items, _) = result {
            assert!(items.is_empty(), "expected empty tail");
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn strlist_join_with_sep() {
        let mut rt = rt();
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let xs = Value::List(
            vec![str_val(&rt, "a"), str_val(&rt, "b"), str_val(&rt, "c")],
            prov,
        );
        let sep = str_val(&rt, ", ");
        let result = dispatch(&mut rt, "strlist_join_native", &[xs, sep], Span::DUMMY)
            .unwrap()
            .unwrap();
        assert_eq!(result.as_str(), Some("a, b, c"));
    }

    #[test]
    fn strlist_contains_found_and_not_found() {
        let mut rt = rt();
        let prov = ProvChain::singleton(
            rt.arena.clone(),
            ProvOp::Synthetic("test".into()),
            Span::DUMMY,
        );
        let xs = Value::List(vec![str_val(&rt, "foo"), str_val(&rt, "bar")], prov);
        let needle_yes = str_val(&rt, "foo");
        let needle_no = str_val(&rt, "baz");
        let yes = dispatch(
            &mut rt,
            "strlist_contains_native",
            &[xs.clone(), needle_yes],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        let no = dispatch(
            &mut rt,
            "strlist_contains_native",
            &[xs, needle_no],
            Span::DUMMY,
        )
        .unwrap()
        .unwrap();
        assert_eq!(yes.as_bool(), Some(true));
        assert_eq!(no.as_bool(), Some(false));
    }
}
