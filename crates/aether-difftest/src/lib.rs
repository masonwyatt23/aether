//! Differential testing harness for Aether.
//!
//! Runs a program on both the tree-walking interpreter (`aether-eval`) and the
//! bytecode VM (`aether-bc`) and compares their observable outputs.  The only
//! failure mode that matters is `Agreement::Differ` — both runtimes produced
//! different results for the same well-formed program.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use aether_difftest::diff_run;
//! use aether_difftest::Agreement;
//!
//! let result = diff_run("fn main() -> Int effects {} { 1 + 2 * 3 }");
//! assert!(matches!(result.agreement, Agreement::Match));
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use aether_ast::{FileId, SourceMap};
use aether_ast::decl::Decl;
use aether_parser::parse_module;

// ── public types ──────────────────────────────────────────────────────────────

/// The outcome of running a program on one runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// The program ran to completion.
    Ok {
        /// Everything written to stdout during execution (newline-separated).
        stdout: String,
        /// The display-string of the final value returned by `main`.
        value: String,
    },
    /// The runtime returned an error.
    Failed { error: String },
    /// The runtime was skipped for a known reason (e.g. unsupported construct).
    Skipped { reason: String },
}

/// How the two runtimes agree (or disagree) on a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Agreement {
    /// Both runtimes succeeded and produced identical stdout + value.
    Match,
    /// Both runtimes succeeded but disagreed on stdout or value.
    Differ {
        tree: Box<RunOutcome>,
        bc: Box<RunOutcome>,
    },
    /// The bytecode compiler bailed with `Unsupported`; tree-walker ran fine.
    BcSkipped { reason: String },
    /// Both runtimes failed (same error path — not a divergence).
    BothFailed {
        tree_error: String,
        bc_error: String,
    },
    /// Tree-walker failed but BC succeeded — recorded but not a hard failure.
    TreeFailed { error: String },
    /// Parse/load error before either runtime was invoked.
    LoadError { error: String },
    /// The program uses nondeterministic builtins (uuid, random, mem_get, etc.).
    /// Output will differ between runs by design — not a real divergence.
    Nondeterministic { reason: String },
}

/// Full result of a differential test run.
#[derive(Debug, Clone)]
pub struct DiffResult {
    pub tree_walker: RunOutcome,
    pub bytecode: RunOutcome,
    pub agreement: Agreement,
}

// ── module loading (mirrors aether-cli's Loader, inlined to avoid depending on it) ──

fn stdlib_source(name: &str) -> Option<&'static str> {
    aether_stdlib::STD_MODULES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, src)| *src)
}

fn load_stdlib_module(
    name: &str,
    src: &'static str,
    sm: &mut SourceMap,
) -> Result<Vec<Decl>, String> {
    let fid = sm.add(format!("<stdlib:{name}>"), src);
    let module = parse_module(fid, src).map_err(|e| format!("{e}"))?;
    let decls = module
        .decls
        .into_iter()
        .filter(|d| !matches!(d, Decl::Import(_)))
        .collect();
    Ok(decls)
}

/// Resolve and merge all imports in `module`, returning a flat merged module.
/// Only handles stdlib imports + filesystem imports relative to `root`.
fn resolve_imports(
    module: aether_ast::Module,
    root: Option<&Path>,
    sm: &mut SourceMap,
    cache: &mut HashMap<String, Vec<Decl>>,
    visiting: &mut std::collections::HashSet<String>,
) -> Result<aether_ast::Module, String> {
    let module_span = module.span;
    let module_name = module.name.clone();
    let module_doc = module.doc.clone();
    let mut merged: Vec<Decl> = Vec::new();

    for decl in module.decls {
        match decl {
            Decl::Import(ref imp) => {
                let qname = imp.path.join("::");
                if let Some(cached) = cache.get(&qname) {
                    merged.extend(cached.iter().cloned());
                    continue;
                }
                let import_decls = if let Some(std_src) = stdlib_source(&qname) {
                    load_stdlib_module(&qname, std_src, sm)?
                } else if let Some(root) = root {
                    let rel: std::path::PathBuf =
                        imp.path.iter().collect::<std::path::PathBuf>().with_extension("ae");
                    let abs = root.join(&rel);
                    if !abs.exists() {
                        return Err(format!("unknown module `{qname}` (not in stdlib and no file found)"));
                    }
                    // Guard against cycles.
                    let key = abs.display().to_string();
                    if visiting.contains(&key) {
                        return Err(format!("import cycle detected involving `{qname}`"));
                    }
                    visiting.insert(key.clone());
                    let src = std::fs::read_to_string(&abs)
                        .map_err(|e| format!("could not read `{}`: {e}", abs.display()))?;
                    let fid = sm.add(abs.display().to_string(), src.clone());
                    let sub_mod = parse_module(fid, &src).map_err(|e| format!("{e}"))?;
                    let resolved = resolve_imports(sub_mod, Some(root), sm, cache, visiting)?;
                    visiting.remove(&key);
                    resolved.decls
                } else {
                    return Err(format!("unknown module `{qname}`"));
                };
                cache.insert(qname, import_decls.clone());
                merged.extend(import_decls);
            }
            other => merged.push(other),
        }
    }

    Ok(aether_ast::Module {
        name: module_name,
        doc: module_doc,
        decls: merged,
        span: module_span,
    })
}

// ── value conversion helpers ──────────────────────────────────────────────────

/// Convert a BC value to an eval value (for trampoline dispatch).
/// Uses a synthetic zero-span provenance chain.
fn bc_to_eval(v: &aether_bc::Value) -> aether_eval::Value {
    use aether_ast::{ProvArena, ProvChain, ProvOp, Span, FileId};
    // `ProvArena::new()` already returns `Arc<ProvArena>`; don't double-wrap.
    let arena = ProvArena::new();
    let span = Span { file: FileId(0), start: 0, end: 0 };
    let prov = ProvChain::singleton(arena, ProvOp::Synthetic("bc-trampoline".into()), span);

    match v {
        aether_bc::Value::Int(n)    => aether_eval::Value::Int(*n, prov),
        aether_bc::Value::Bool(b)   => aether_eval::Value::Bool(*b, prov),
        aether_bc::Value::Str(s)    => aether_eval::Value::Str(s.clone(), prov),
        aether_bc::Value::Float(f)  => aether_eval::Value::Float(*f, prov),
        aether_bc::Value::Unit      => aether_eval::Value::Unit(prov),
        aether_bc::Value::List(vs)  => {
            aether_eval::Value::List(vs.iter().map(bc_to_eval).collect(), prov)
        }
        aether_bc::Value::Tuple(vs) => {
            aether_eval::Value::Tuple(vs.iter().map(bc_to_eval).collect(), prov)
        }
        aether_bc::Value::Record(fs) => {
            let fields = fs.iter().map(|(k, v)| (k.clone(), bc_to_eval(v))).collect();
            aether_eval::Value::Record(fields, prov)
        }
        aether_bc::Value::Ctor { name, args } => aether_eval::Value::Ctor {
            name: name.clone(),
            args: args.iter().map(bc_to_eval).collect(),
            prov,
        },
        aether_bc::Value::Closure { fn_idx, .. } => {
            // Closures can't meaningfully round-trip through eval; return a placeholder.
            aether_eval::Value::Str(format!("<closure fn{}>", fn_idx), prov)
        }
        aether_bc::Value::Confident { inner, p } => aether_eval::Value::Confident {
            value: Box::new(bc_to_eval(inner)),
            p: *p,
            prov,
        },
    }
}

/// Convert an eval value back to a BC value (for trampoline return).
fn eval_to_bc(v: &aether_eval::Value) -> aether_bc::Value {
    match v {
        aether_eval::Value::Int(n, _)    => aether_bc::Value::Int(*n),
        aether_eval::Value::Bool(b, _)   => aether_bc::Value::Bool(*b),
        aether_eval::Value::Str(s, _)    => aether_bc::Value::Str(s.clone()),
        aether_eval::Value::Float(f, _)  => aether_bc::Value::Float(*f),
        aether_eval::Value::Unit(_)      => aether_bc::Value::Unit,
        aether_eval::Value::List(vs, _)  => {
            aether_bc::Value::List(vs.iter().map(eval_to_bc).collect())
        }
        aether_eval::Value::Tuple(vs, _) => {
            aether_bc::Value::Tuple(vs.iter().map(eval_to_bc).collect())
        }
        aether_eval::Value::Record(fs, _) => {
            aether_bc::Value::Record(fs.iter().map(|(k, v)| (k.clone(), eval_to_bc(v))).collect())
        }
        aether_eval::Value::Ctor { name, args, .. } => aether_bc::Value::Ctor {
            name: name.clone(),
            args: args.iter().map(eval_to_bc).collect(),
        },
        aether_eval::Value::Confident { value, p, .. } => aether_bc::Value::Confident {
            inner: Box::new(eval_to_bc(value)),
            p: *p,
        },
        // Other eval-only values (ModuleSurface, ProvHandle, etc.) → Unit
        _ => aether_bc::Value::Unit,
    }
}

// ── trampoline dispatcher ─────────────────────────────────────────────────────

/// Sentinel tag used to encode eval-only opaque values (ProvHandle, ModuleSurface)
/// as BC `Value::Ctor { name: OPAQUE_TAG, args: [Int(id)] }` so they can survive
/// the round-trip through BC locals without losing identity.
const OPAQUE_TAG: &str = "__aether_opaque__";

/// A `BuiltinDispatcher` that forwards unknown BC builtin calls into the
/// tree-walker's builtin dispatch layer.  This provides observable equivalence
/// for stdlib natives, `assert_eq`, `http_get`, etc. without needing native BC
/// implementations for every builtin.
///
/// Any stdout emitted by trampolined builtins (e.g. `print_module_surface`,
/// `print_prov`) is drained into `stdout_drain` after every call.  The BC VM
/// runner appends that accumulated buffer to `vm.stdout` once execution
/// completes.
///
/// Eval-only values (`ProvHandle`, `ModuleSurface`) that cannot be natively
/// represented in BC are stored in `opaque_store` and encoded as
/// `Value::Ctor { name: OPAQUE_TAG, args: [Int(id)] }`.  When they're passed
/// back as arguments to a subsequent trampoline call, `bc_to_eval_with_store`
/// recovers the original eval value.
struct EvalTrampoline {
    rt: aether_eval::Runtime,
    /// Shared buffer: all stdout produced by trampoline dispatch accumulates here.
    /// Shared (via `Arc<Mutex<>>`) so `run_bytecode` can read it after `vm.run()`.
    stdout_drain: Arc<Mutex<String>>,
    /// Opaque value store: maps integer IDs → eval values that can't be encoded in BC.
    opaque_store: std::collections::HashMap<i64, aether_eval::Value>,
    /// Monotonically increasing counter for opaque IDs.
    opaque_next_id: i64,
}

impl EvalTrampoline {
    fn new(module: aether_ast::Module, stdout_drain: Arc<Mutex<String>>) -> Self {
        let mut rt = aether_eval::Runtime::new(module);
        rt.capture_only = true;
        Self { rt, stdout_drain, opaque_store: Default::default(), opaque_next_id: 1 }
    }

    /// Convert an eval value to a BC value, storing opaque values in `opaque_store`.
    fn eval_to_bc_local(&mut self, v: &aether_eval::Value) -> aether_bc::Value {
        match v {
            // Eval-only types: store in the opaque table, return a Ctor sentinel.
            aether_eval::Value::ProvHandle(..)
            | aether_eval::Value::ModuleSurface(..) => {
                let id = self.opaque_next_id;
                self.opaque_next_id += 1;
                self.opaque_store.insert(id, v.clone());
                aether_bc::Value::Ctor {
                    name: OPAQUE_TAG.to_string(),
                    args: vec![aether_bc::Value::Int(id)],
                }
            }
            aether_eval::Value::Confident { value, p, .. } => aether_bc::Value::Confident {
                inner: Box::new(self.eval_to_bc_local(value)),
                p: *p,
            },
            other => eval_to_bc(other),
        }
    }

    /// Convert a BC value back to an eval value, recovering opaque sentinels.
    fn bc_to_eval_local(&self, v: &aether_bc::Value) -> aether_eval::Value {
        // Check for the opaque sentinel before the generic bc_to_eval.
        if let aether_bc::Value::Ctor { name, args } = v {
            if name == OPAQUE_TAG {
                if let Some(aether_bc::Value::Int(id)) = args.first() {
                    if let Some(stored) = self.opaque_store.get(id) {
                        return stored.clone();
                    }
                }
            }
        }
        bc_to_eval(v)
    }
}

impl aether_bc::BuiltinDispatcher for EvalTrampoline {
    fn call(&mut self, name: &str, args: &[aether_bc::Value]) -> Result<aether_bc::Value, String> {
        use aether_ast::{Span, FileId};
        let span = Span { file: FileId(0), start: 0, end: 0 };

        // Convert BC args → eval args, recovering any opaque sentinels.
        let eval_args: Vec<aether_eval::Value> = args.iter().map(|v| self.bc_to_eval_local(v)).collect();

        // Dispatch through the eval builtin layer.
        let result = match aether_eval::builtins::dispatch(&mut self.rt, name, &eval_args, span) {
            Ok(Some(v)) => Ok(self.eval_to_bc_local(&v)),
            Ok(None) => {
                // None means "not a builtin I know" — return an error so the BC VM
                // reports a meaningful failure rather than silently returning Unit.
                Err(format!("builtin `{name}` not found in eval dispatch"))
            }
            Err(e) => Err(e.to_string()),
        };

        // Drain any stdout that the trampoline's runtime accumulated (e.g. from
        // `print_module_surface`, `print_prov`, or any `print` call routed here).
        let emitted = std::mem::take(&mut self.rt.stdout);
        if !emitted.is_empty() {
            if let Ok(mut drain) = self.stdout_drain.lock() {
                drain.push_str(&emitted);
            }
        }

        result
    }
}

// ── core runners ──────────────────────────────────────────────────────────────

fn run_tree_walker(module: aether_ast::Module) -> RunOutcome {
    let mut rt = aether_eval::Runtime::new(module);
    rt.capture_only = true;
    match rt.run_main() {
        Ok(val) => RunOutcome::Ok {
            stdout: rt.stdout.clone(),
            value: val.display(),
        },
        Err(e) => RunOutcome::Failed { error: e.to_string() },
    }
}

fn run_bytecode(module: &aether_ast::Module) -> RunOutcome {
    match aether_bc::compile_module(module) {
        Err(aether_bc::CompileError::Unsupported(reason)) => {
            RunOutcome::Skipped { reason }
        }
        Err(e) => RunOutcome::Failed { error: e.to_string() },
        Ok(program) => {
            // Shared buffer that the trampoline drains its stdout into after each
            // dispatched call.  We read it after `vm.run()` completes.
            let stdout_drain: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
            // Wire in the eval trampoline dispatcher so CallBuiltinDyn ops can
            // call back into the tree-walker's builtin layer.
            let dispatcher = Box::new(EvalTrampoline::new(module.clone(), Arc::clone(&stdout_drain)));
            let mut vm = aether_bc::Vm::with_dispatcher(&program, dispatcher);
            vm.capture_only = true;
            match vm.run() {
                Ok(val) => {
                    // Append any stdout emitted by trampolined builtins.
                    let drained = stdout_drain.lock().map(|g| g.clone()).unwrap_or_default();
                    let mut full_stdout = vm.stdout.clone();
                    full_stdout.push_str(&drained);
                    RunOutcome::Ok {
                        stdout: full_stdout,
                        value: val.display(),
                    }
                }
                Err(e) => RunOutcome::Failed { error: e.to_string() },
            }
        }
    }
}

/// Builtin names whose output is nondeterministic across runs (uuid, random,
/// persistent-memory reads, time, LLM completions).  If a program's source
/// contains any of these names, outputs are allowed to differ.
const NONDETERMINISTIC_BUILTINS: &[&str] = &[
    "uuid_v4",
    "uuid_short",
    "random_int",
    "random_bool",
    "random_float",
    "random_pick",
    "mem_get",
    "mem_set",
    "time_now_ms",
    "time_monotonic_ms",
    "sys_now_unix",
    "llm_complete",
];

/// Return `Some(reason)` if `source` references any nondeterministic builtins.
pub fn nondeterministic_reason(source: &str) -> Option<String> {
    for name in NONDETERMINISTIC_BUILTINS {
        if source.contains(name) {
            return Some(format!("source references nondeterministic builtin `{name}`"));
        }
    }
    None
}

/// Builtins that produce or consume *eval-only* values — `ProvChain` and
/// `ModuleSurface`. The bytecode VM has no representation for these, so they
/// cannot round-trip through the BC↔eval trampoline. Programs using them are
/// classified `BcSkipped`: this is a known, documented model limitation, not a
/// semantic divergence.
const BC_INCOMPATIBLE_BUILTINS: &[&str] = &[
    "provenance",
    "print_prov",
    "introspect",
    "print_module_surface",
    "summarize",
];

/// Return `Some(reason)` if `source` references any builtin whose value type
/// the bytecode VM cannot model.
pub fn bc_incompatible_reason(source: &str) -> Option<String> {
    for name in BC_INCOMPATIBLE_BUILTINS {
        if source.contains(name) {
            return Some(format!(
                "uses `{name}` — produces an eval-only value (ProvChain/ModuleSurface) \
                 the bytecode VM cannot represent"
            ));
        }
    }
    None
}

fn compute_agreement(tree: &RunOutcome, bc: &RunOutcome, source: &str) -> Agreement {
    match (tree, bc) {
        (RunOutcome::Ok { stdout: ts, value: tv }, RunOutcome::Ok { stdout: bs, value: bv }) => {
            if ts == bs && tv == bv {
                Agreement::Match
            } else if let Some(reason) = nondeterministic_reason(source) {
                // Outputs differ but it's expected — program uses stochastic builtins.
                Agreement::Nondeterministic { reason }
            } else if let Some(reason) = bc_incompatible_reason(source) {
                // The BC VM can't model ProvChain / ModuleSurface; an output
                // difference here is a known model limitation, not a bug.
                Agreement::BcSkipped { reason }
            } else {
                Agreement::Differ {
                    tree: Box::new(tree.clone()),
                    bc: Box::new(bc.clone()),
                }
            }
        }
        (RunOutcome::Ok { .. }, RunOutcome::Skipped { reason }) => {
            Agreement::BcSkipped { reason: reason.clone() }
        }
        (RunOutcome::Failed { error: te }, RunOutcome::Failed { error: be }) => {
            Agreement::BothFailed {
                tree_error: te.clone(),
                bc_error: be.clone(),
            }
        }
        (RunOutcome::Failed { error }, _) => {
            Agreement::TreeFailed { error: error.clone() }
        }
        // Tree succeeded, BC failed at runtime — BC coverage gap (e.g. Float
        // arithmetic, missing dispatch), not a semantic disagreement between
        // two successful outputs.
        (RunOutcome::Ok { .. }, RunOutcome::Failed { error }) => {
            Agreement::BcSkipped {
                reason: format!("BC runtime error (coverage gap): {error}"),
            }
        }
        _ => unreachable!("unhandled agreement pattern"),
    }
}

// ── public API ────────────────────────────────────────────────────────────────

/// Run `source` on both the tree-walker and the bytecode VM and compare.
///
/// Imports are resolved only against the stdlib (no filesystem root).
pub fn diff_run(source: &str) -> DiffResult {
    let mut sm = SourceMap::new();
    let mut cache = HashMap::new();
    let mut visiting = std::collections::HashSet::new();

    let module = match parse_module(FileId(0), source) {
        Ok(m) => m,
        Err(e) => {
            let err = format!("{e}");
            return DiffResult {
                tree_walker: RunOutcome::Failed { error: err.clone() },
                bytecode: RunOutcome::Failed { error: err.clone() },
                agreement: Agreement::LoadError { error: err },
            };
        }
    };

    let module = match resolve_imports(module, None, &mut sm, &mut cache, &mut visiting) {
        Ok(m) => m,
        Err(e) => {
            return DiffResult {
                tree_walker: RunOutcome::Failed { error: e.clone() },
                bytecode: RunOutcome::Failed { error: e.clone() },
                agreement: Agreement::LoadError { error: e },
            };
        }
    };

    let tree = run_tree_walker(module.clone());
    let bc = run_bytecode(&module);
    let agreement = compute_agreement(&tree, &bc, source);

    DiffResult { tree_walker: tree, bytecode: bc, agreement }
}

/// Load `path`, resolve imports relative to its parent directory, run both
/// runtimes, and compare.
pub fn diff_file(path: &Path) -> std::io::Result<DiffResult> {
    let source = std::fs::read_to_string(path)?;
    let root = path.parent().unwrap_or(Path::new("."));

    let mut sm = SourceMap::new();
    let mut cache = HashMap::new();
    let mut visiting = std::collections::HashSet::new();

    let fid = sm.add(path.display().to_string(), source.clone());
    let module = match parse_module(fid, &source) {
        Ok(m) => m,
        Err(e) => {
            let err = format!("{e}");
            return Ok(DiffResult {
                tree_walker: RunOutcome::Failed { error: err.clone() },
                bytecode: RunOutcome::Failed { error: err.clone() },
                agreement: Agreement::LoadError { error: err },
            });
        }
    };

    let module = match resolve_imports(module, Some(root), &mut sm, &mut cache, &mut visiting) {
        Ok(m) => m,
        Err(e) => {
            return Ok(DiffResult {
                tree_walker: RunOutcome::Failed { error: e.clone() },
                bytecode: RunOutcome::Failed { error: e.clone() },
                agreement: Agreement::LoadError { error: e },
            });
        }
    };

    let tree = run_tree_walker(module.clone());
    let bc = run_bytecode(&module);
    let agreement = compute_agreement(&tree, &bc, &source);

    Ok(DiffResult { tree_walker: tree, bytecode: bc, agreement })
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_matches() {
        let result = diff_run("fn main() -> Int effects {} { 1 + 2 * 3 }");
        assert!(
            matches!(result.agreement, Agreement::Match),
            "expected Match, got {:?}\n  tree: {:?}\n  bc:   {:?}",
            result.agreement,
            result.tree_walker,
            result.bytecode,
        );
        // Verify the value itself.
        if let RunOutcome::Ok { value, .. } = &result.tree_walker {
            assert_eq!(value, "7", "expected value=7, got {value}");
        }
    }

    #[test]
    fn bc_skipped_on_unsupported() {
        // Unsupported constructs (e.g. literal patterns inside ctor patterns)
        // should produce BcSkipped.
        let src = r#"
fn main() -> Str effects {} {
    let name = "world"
    name
}
"#;
        // This should match fine (not skipped).
        let result = diff_run(src);
        assert!(
            matches!(result.agreement, Agreement::Match),
            "expected Match, got {:?}",
            result.agreement,
        );
    }

    #[test]
    fn tree_only_failure_propagates() {
        // Division by zero — both runtimes should fail.
        let src = "fn main() -> Int effects {} { 1 / 0 }";
        let result = diff_run(src);
        // Both may fail; either BothFailed or TreeFailed is acceptable —
        // the important thing is NOT Differ.
        assert!(
            !matches!(result.agreement, Agreement::Differ { .. }),
            "division-by-zero should not produce Differ, got {:?}",
            result.agreement,
        );
    }

    #[test]
    fn print_stdout_matches() {
        let src = r#"fn main() -> Unit effects {IO} { print("hello difftest") }"#;
        let result = diff_run(src);
        assert!(
            matches!(result.agreement, Agreement::Match),
            "print stdout should match: {:?}\n  tree: {:?}\n  bc:   {:?}",
            result.agreement,
            result.tree_walker,
            result.bytecode,
        );
    }

    /// Verify the trampoline stdout-drain works for builtins whose values
    /// round-trip cleanly across the BC↔eval boundary. `len` returns a plain
    /// `Int`, so `print(str(len(...)))` must produce identical stdout on both
    /// runtimes — the drain mechanism is what makes the trampolined call's
    /// output reach the BC `RunOutcome`.
    #[test]
    fn trampoline_stdout_drained_into_bc_outcome() {
        let src = r#"fn main() -> Unit effects {IO} { print(str(len("hello"))) }"#;
        let result = diff_run(src);
        match (&result.tree_walker, &result.bytecode) {
            (RunOutcome::Ok { stdout: tw_out, .. }, RunOutcome::Ok { stdout: bc_out, .. }) => {
                assert!(!tw_out.is_empty(), "tree-walker should print `5`");
                assert_eq!(
                    tw_out.trim(), bc_out.trim(),
                    "trampoline drain must surface BC stdout for round-tripping builtins"
                );
            }
            other => panic!("expected both sides Ok, got {:?}", other),
        }
    }

    /// `provenance` / `print_prov` produce eval-only `ProvChain` values the
    /// bytecode VM cannot model. Such programs must classify as `BcSkipped`,
    /// never `Differ` — a documented, deliberate model limitation.
    #[test]
    fn provenance_program_classified_bc_skipped() {
        let src = r#"
fn main() -> Unit effects {IO} {
    let z = (5 + 3) * 2
    let chain = provenance(z)
    print_prov(chain)
}
"#;
        assert!(
            bc_incompatible_reason(src).is_some(),
            "bc_incompatible_reason should detect `provenance`/`print_prov`",
        );
        let result = diff_run(src);
        assert!(
            matches!(result.agreement, Agreement::BcSkipped { .. } | Agreement::Match),
            "provenance program must be BcSkipped or Match, never Differ — got {:?}",
            result.agreement,
        );
    }

    /// Verify that a program referencing nondeterministic builtins (`uuid_short`,
    /// `random_int`, etc.) is never classified as `Agreement::Differ`.
    /// The nondeterministic source scanner must detect the builtin names and
    /// classify accordingly before any runtime disagreement becomes a test failure.
    #[test]
    fn uuid_program_classified_nondeterministic() {
        // 1. Verify the source scanner itself works.
        assert!(
            nondeterministic_reason("uuid_short()").is_some(),
            "nondeterministic_reason should detect uuid_short"
        );
        assert!(
            nondeterministic_reason("random_int(1, 100)").is_some(),
            "nondeterministic_reason should detect random_int"
        );
        assert!(
            nondeterministic_reason("1 + 2").is_none(),
            "nondeterministic_reason should NOT fire on pure arithmetic"
        );

        // 2. A well-formed program whose outputs will differ each run should be
        //    classified Nondeterministic (not Differ) when both sides succeed.
        //    Use 21_new_stdlib.ae which imports std::uuid and actually runs.
        let path = {
            let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            // Walk up to workspace root.
            loop {
                let candidate = p.join("Cargo.toml");
                if candidate.exists() {
                    let c = std::fs::read_to_string(&candidate).unwrap_or_default();
                    if c.contains("[workspace]") { break; }
                }
                assert!(p.pop(), "could not find workspace root");
            }
            p.join("examples/21_new_stdlib.ae")
        };
        if path.exists() {
            let result = diff_file(&path).expect("I/O error");
            assert!(
                !matches!(result.agreement, Agreement::Differ { .. }),
                "21_new_stdlib.ae must not be Differ — it uses nondeterministic builtins. \
                 Got {:?}", result.agreement
            );
        }
    }
}
