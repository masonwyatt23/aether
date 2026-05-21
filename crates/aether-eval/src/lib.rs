//! Aether interpreter.
//!
//! Tree-walking evaluator over `aether_ast`. Every produced runtime value
//! carries a `ProvChain` — provenance is built automatically as ops are
//! applied. `@no_prov` on a function declaration disables provenance for
//! values computed inside its body (declared but not yet wired in MVP).
//!
//! The interpreter is single-threaded and synchronous. Effects declared at
//! call sites are not enforced at runtime — the static type checker already
//! does that. The interpreter simply executes them.

#![allow(clippy::module_inception)]

pub mod builtins;
pub mod env;
pub mod tools;
pub mod value;

pub use builtins::{ExportEntry, ModuleSurface};
pub use tools::{ToolFn, ToolRegistry};
pub use value::Value;

use aether_ast::*;
use std::sync::Arc;
use thiserror::Error;

use env::Env;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("unbound variable `{0}`")]
    Unbound(String),
    #[error("type error at runtime: {0}")]
    TypeError(String),
    #[error("user error: {0}")]
    User(String),
    #[error("division by zero")]
    DivByZero,
    #[error("function `{0}` not defined")]
    UndefinedFn(String),
}

pub type EResult<T> = Result<T, EvalError>;

/// Outcome of one in-language `test "..." { ... }` block.
#[derive(Debug, Clone)]
pub enum TestOutcome {
    Pass,
    Fail(String),
}

#[derive(Debug, Clone)]
pub struct TestReport {
    /// Human-readable name (the original string literal, or the underscored fn name).
    pub name: String,
    /// The synthesized function name (`test__<sanitized>`).
    pub fn_name: String,
    pub outcome: TestOutcome,
}

impl TestReport {
    pub fn passed(&self) -> bool {
        matches!(self.outcome, TestOutcome::Pass)
    }
}

#[derive(Debug, Clone)]
pub struct BenchReport {
    pub name: String,
    pub fn_name: String,
    pub iters: u32,
    pub total_us: u64,
    pub mean_us: f64,
    pub outcome: TestOutcome,
}

impl BenchReport {
    pub fn passed(&self) -> bool {
        matches!(self.outcome, TestOutcome::Pass)
    }
}

/// Result of [`Runtime::eval_tail`]: either a fully evaluated value, or a
/// tail-call token carrying the target function name and pre-evaluated args.
///
/// Using an explicit enum (rather than `Result` or `Option`) keeps the
/// trampoline branch in `eval_fn` legible and avoids any allocation.
enum TailStep {
    /// The expression evaluated to a final value — no tail-call detected.
    Done(Value),
    /// The tail position was a direct call to a user-defined function.
    /// `args` have already been evaluated; `prov_parents` and `span` are
    /// retained so callers can construct provenance nodes if needed.
    TailCall {
        fn_name: String,
        args: Vec<Value>,
        /// Provenance node indices of the evaluated arguments (reserved for
        /// future use — a caller could attach a ProvOp::Call node per hop).
        #[allow(dead_code)]
        prov_parents: Vec<usize>,
        #[allow(dead_code)]
        span: Span,
    },
}

/// Holds program state across evaluation.
pub struct Runtime {
    pub module: Module,
    pub arena: Arc<ProvArena>,
    /// Captured stdout (for tests/inspection). Real `print` also writes to host stdout.
    pub stdout: String,
    /// Suppress writing to host stdout (used by tests).
    pub capture_only: bool,
    /// Optional `assume`d facts gathered during evaluation (surfaced via provenance).
    pub assumed: Vec<Expr>,
    /// Registered tool handlers. Dispatched before built-in fallbacks so a host
    /// application can override `llm_complete`, `http_get`, etc.
    pub tools: ToolRegistry,
    /// Content-hash cache for `summarize(scope, budget)`. Spec §6 requires that
    /// repeated calls with the same `(scope, budget, module_hash)` triple return
    /// the same string in O(1).
    pub summarize_cache: std::collections::HashMap<u64, String>,
    /// Accumulated (label, value) pairs collected by `snap_expect` during a
    /// single snap block execution. Cleared before each block runs.
    pub snapshot_buffer: Vec<(String, String)>,
}

impl Runtime {
    pub fn new(module: Module) -> Self {
        let mut tools = ToolRegistry::new();
        tools::install_defaults(&mut tools);
        Self {
            module,
            arena: ProvArena::new(),
            stdout: String::new(),
            capture_only: false,
            assumed: Vec::new(),
            tools,
            summarize_cache: std::collections::HashMap::new(),
            snapshot_buffer: Vec::new(),
        }
    }

    pub fn capture(&mut self) -> &mut Self {
        self.capture_only = true;
        self
    }

    /// Discover and run every `bench "..." { ... }` block, iterating each one
    /// `iters` times and reporting per-iter mean time. Failures are surfaced
    /// the same way as test failures.
    pub fn run_benches(&mut self, iters: u32) -> Vec<BenchReport> {
        let bench_fns: Vec<FnDecl> = self
            .module
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Fn(f) if f.name.starts_with("bench__") => Some(f.clone()),
                _ => None,
            })
            .collect();
        let mut reports = Vec::with_capacity(bench_fns.len());
        for f in bench_fns {
            let display_name = f
                .doc
                .clone()
                .unwrap_or_else(|| f.name.trim_start_matches("bench__").replace('_', " "));
            // Warm-up — one run, errors abort the benchmark for this name.
            {
                let mut env = Env::root();
                if let Err(e) = self.eval_fn(&f, vec![], &mut env) {
                    reports.push(BenchReport {
                        name: display_name,
                        fn_name: f.name.clone(),
                        iters: 0,
                        total_us: 0,
                        mean_us: 0.0,
                        outcome: TestOutcome::Fail(e.to_string()),
                    });
                    continue;
                }
            }
            let start = std::time::Instant::now();
            let mut err: Option<String> = None;
            for _ in 0..iters {
                let mut env = Env::root();
                if let Err(e) = self.eval_fn(&f, vec![], &mut env) {
                    err = Some(e.to_string());
                    break;
                }
            }
            let total = start.elapsed();
            let total_us = total.as_micros() as u64;
            let mean_us = if iters == 0 {
                0.0
            } else {
                total_us as f64 / iters as f64
            };
            reports.push(BenchReport {
                name: display_name,
                fn_name: f.name,
                iters,
                total_us,
                mean_us,
                outcome: match err {
                    None => TestOutcome::Pass,
                    Some(e) => TestOutcome::Fail(e),
                },
            });
        }
        reports
    }

    /// Discover and run every `test "..." { ... }` block in the module
    /// (desugared by the parser to `fn test__<sanitized>() effects {Throw}`).
    /// Returns one report row per test.
    pub fn run_tests(&mut self) -> Vec<TestReport> {
        let test_fns: Vec<FnDecl> = self
            .module
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Fn(f) if f.name.starts_with("test__") => Some(f.clone()),
                _ => None,
            })
            .collect();
        let mut reports = Vec::with_capacity(test_fns.len());
        for f in test_fns {
            let display_name = f
                .doc
                .clone()
                .unwrap_or_else(|| f.name.trim_start_matches("test__").replace('_', " "));
            let mut env = Env::root();
            let result = self.eval_fn(&f, vec![], &mut env);
            reports.push(TestReport {
                name: display_name,
                fn_name: f.name,
                outcome: match result {
                    Ok(_) => TestOutcome::Pass,
                    Err(e) => TestOutcome::Fail(e.to_string()),
                },
            });
        }
        reports
    }

    /// Discover and run every `snap "..." { ... }` block.
    ///
    /// `snap_path` is the path to the source file — the golden file is placed
    /// alongside it as `<stem>.snap`.
    ///
    /// `update` — when true, overwrite (or create) the golden file instead of
    /// comparing. First-run (file absent) always writes and reports "captured".
    pub fn run_snapshots(&mut self, snap_path: &std::path::Path, update: bool) -> Vec<SnapReport> {
        let snap_fns: Vec<FnDecl> = self
            .module
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Fn(f) if f.name.starts_with("snap__") => Some(f.clone()),
                _ => None,
            })
            .collect();

        // Derive the golden-file path: replace extension with `.snap`.
        let golden_path = snap_path.with_extension("snap");

        // Parse existing golden file (if any) into a map:
        //   section_name -> Vec<(label, value)>
        let existing = load_snap_file(&golden_path);
        let file_existed = golden_path.exists();

        let mut reports = Vec::with_capacity(snap_fns.len());
        // We'll accumulate the new golden content for all sections.
        let mut new_golden: std::collections::BTreeMap<String, Vec<(String, String)>> =
            std::collections::BTreeMap::new();

        for f in snap_fns {
            let display_name = f
                .doc
                .clone()
                .unwrap_or_else(|| f.name.trim_start_matches("snap__").replace('_', " "));
            let section_key = f.name.trim_start_matches("snap__").to_string();

            // Reset buffer, run the block.
            self.snapshot_buffer.clear();
            let mut env = env::Env::root();
            let run_result = self.eval_fn(&f, vec![], &mut env);
            if let Err(e) = run_result {
                reports.push(SnapReport {
                    name: display_name,
                    fn_name: f.name.clone(),
                    status: SnapStatus::Error(e.to_string()),
                });
                // Keep existing section in golden if present.
                if let Some(existing_section) = existing.get(&section_key) {
                    new_golden.insert(section_key, existing_section.clone());
                }
                continue;
            }
            let captured: Vec<(String, String)> = std::mem::take(&mut self.snapshot_buffer);

            new_golden.insert(section_key.clone(), captured.clone());

            if !file_existed || update {
                // First run or explicit update — write and report captured.
                reports.push(SnapReport {
                    name: display_name,
                    fn_name: f.name.clone(),
                    status: SnapStatus::Captured,
                });
            } else {
                // Verify mode: compare against existing golden.
                let golden_entries = existing.get(&section_key).cloned().unwrap_or_default();
                let mismatches = diff_entries(&golden_entries, &captured);
                if mismatches.is_empty() {
                    reports.push(SnapReport {
                        name: display_name,
                        fn_name: f.name.clone(),
                        status: SnapStatus::Pass,
                    });
                } else {
                    reports.push(SnapReport {
                        name: display_name,
                        fn_name: f.name.clone(),
                        status: SnapStatus::Mismatch(mismatches),
                    });
                }
            }
        }

        // Write golden file when first run or update mode.
        if !file_existed || update {
            // Merge: sections not re-run stay as-is from existing.
            for (k, v) in &existing {
                new_golden.entry(k.clone()).or_insert_with(|| v.clone());
            }
            let _ = write_snap_file(&golden_path, &new_golden);
        }

        reports
    }

    /// Evaluate the module's `main()` function (if present).
    pub fn run_main(&mut self) -> EResult<Value> {
        let fn_decl = self
            .module
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Fn(f) if f.name == "main" => Some(f.clone()),
                _ => None,
            })
            .ok_or_else(|| EvalError::UndefinedFn("main".into()))?;
        let mut env = Env::root();
        self.eval_fn(&fn_decl, vec![], &mut env)
    }

    /// Evaluate an arbitrary expression in a fresh root environment.
    pub fn eval_root(&mut self, e: &Expr) -> EResult<Value> {
        let mut env = Env::root();
        let lets: Vec<LetDecl> = self
            .module
            .decls
            .iter()
            .filter_map(|d| {
                if let Decl::Let(l) = d {
                    Some(l.clone())
                } else {
                    None
                }
            })
            .collect();
        for l in lets {
            let v = self.eval(&l.value, &mut env)?;
            env.bind(l.name.clone(), v);
        }
        self.eval(e, &mut env)
    }

    pub fn eval_fn(&mut self, f: &FnDecl, args: Vec<Value>, env: &mut Env) -> EResult<Value> {
        // Trampoline loop for tail-call optimisation (TCO).
        //
        // When eval_tail detects that the tail position of a function body is a
        // direct call to a user-defined function, it returns TailStep::TailCall
        // instead of recursing.  We rebind the parameters and loop, avoiding a
        // new Rust stack frame for every iteration.  Non-tail calls inside the
        // body still recurse through eval normally — that is both correct and
        // unavoidable.
        let mut current_fn = f.clone();
        let mut current_args = args;

        loop {
            let mut local = env.child();
            for (p, v) in current_fn.params.iter().zip(current_args.into_iter()) {
                local.bind(p.name.clone(), v);
            }

            match self.eval_tail(&current_fn.body, &mut local)? {
                TailStep::Done(v) => return Ok(v),
                TailStep::TailCall {
                    fn_name,
                    args: next_args,
                    ..
                } => {
                    // Look up the target user function and loop.
                    match self.module.decls.iter().find_map(|d| {
                        if let Decl::Fn(f) = d {
                            if f.name == fn_name {
                                Some(f.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }) {
                        Some(target_fn) => {
                            if target_fn.params.len() != next_args.len() {
                                return Err(EvalError::TypeError(format!(
                                    "{fn_name}: expected {} args, got {}",
                                    target_fn.params.len(),
                                    next_args.len()
                                )));
                            }
                            current_fn = target_fn;
                            current_args = next_args;
                            // Continue the loop — no new Rust stack frame.
                        }
                        None => return Err(EvalError::UndefinedFn(fn_name)),
                    }
                }
            }
        }
    }

    /// Walk `expr` to its tail position and decide whether it is a direct call
    /// to a user-defined function that can be optimised by the trampoline.
    ///
    /// # Returns
    /// - [`TailStep::TailCall`] when the tail position is a `Call` whose
    ///   callee is a plain identifier naming a module-level user function (not
    ///   shadowed by a closure and not a builtin).  Arguments are already
    ///   evaluated at this point.
    /// - [`TailStep::Done`] for every other case; the expression is evaluated
    ///   via the normal [`Runtime::eval`] path.
    ///
    /// # Tail positions handled
    /// - The expression itself is `Expr::Call` → tail-call candidate.
    /// - `Block { tail: Some(e) }` → recurse into `e` after executing stmts.
    /// - `If { cond, then_branch, else_branch }` → evaluate cond, recurse
    ///   into the chosen branch.
    /// - `Match { .. }` → evaluate scrutinee + guards, recurse into the
    ///   matched arm's body.
    /// - `Annot { expr }` → transparent wrapper, recurse.
    ///
    /// Everything else falls through to a plain `eval` call (`Done`).
    ///
    /// # Provenance note
    /// For a TCO'd tail call the `ProvOp::Call` node is recorded by
    /// `eval_call` on the *next* iteration (because each loop body re-enters
    /// `eval_tail` → `eval_call`).  For very long loops the provenance chain
    /// will be long but structurally correct — every logical call is recorded.
    fn eval_tail(&mut self, expr: &Expr, env: &mut Env) -> EResult<TailStep> {
        match expr {
            // ── Block: execute stmts, then recurse into tail expr. ──────────
            Expr::Block { stmts, tail, span } => {
                let mut inner = env.child();
                for st in stmts {
                    match st {
                        Stmt::Let {
                            pat: Pattern::Var(name, _),
                            value,
                            ..
                        } => {
                            let v = self.eval(value, &mut inner)?;
                            inner.bind(name.clone(), v);
                        }
                        Stmt::Let { value, .. } => {
                            self.eval(value, &mut inner)?;
                        }
                        Stmt::Expr(e) => {
                            self.eval(e, &mut inner)?;
                        }
                    }
                }
                if let Some(t) = tail {
                    self.eval_tail(t, &mut inner)
                } else {
                    Ok(TailStep::Done(Value::unit(self.arena.clone(), *span)))
                }
            }

            // ── If: evaluate cond, recurse into the chosen branch. ──────────
            Expr::If {
                cond,
                then_branch,
                else_branch,
                ..
            } => {
                let cv = self.eval(cond, env)?;
                let cb = cv
                    .as_bool()
                    .ok_or_else(|| EvalError::TypeError("if condition must be Bool".into()))?;
                if cb {
                    self.eval_tail(then_branch, env)
                } else {
                    self.eval_tail(else_branch, env)
                }
            }

            // ── Match: find the matched arm, recurse into its body. ─────────
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => {
                let scrut = self.eval(scrutinee, env)?;
                for arm in arms {
                    let mut local = env.child();
                    if pattern_match(&arm.pat, &scrut, &mut local) {
                        if let Some(guard) = &arm.guard {
                            let gv = self.eval(guard, &mut local)?;
                            if gv.as_bool() != Some(true) {
                                continue;
                            }
                        }
                        return match self.eval_tail(&arm.body, &mut local)? {
                            TailStep::Done(v) => {
                                // Wrap with match provenance, same as eval_match.
                                let prov = ProvChain::extend(
                                    self.arena.clone(),
                                    ProvOp::Synthetic("match".into()),
                                    *span,
                                    vec![scrut.prov().head, v.prov().head],
                                );
                                Ok(TailStep::Done(v.with_prov(prov)))
                            }
                            step @ TailStep::TailCall { .. } => Ok(step),
                        };
                    }
                }
                Err(EvalError::User(format!(
                    "non-exhaustive match at {span:?}: no arm matched value of type {}",
                    scrut.type_name()
                )))
            }

            // ── Annot: transparent wrapper. ──────────────────────────────────
            Expr::Annot { expr, .. } => self.eval_tail(expr, env),

            // ── Call: check whether the callee names a user function. ────────
            Expr::Call { callee, args, span } => {
                if let Expr::Var(name, _) = callee.as_ref() {
                    // Only optimise if the name is NOT shadowed by a closure.
                    let shadowed = matches!(env.lookup(name), Some(Value::Closure { .. }));
                    if !shadowed {
                        let is_user_fn = self
                            .module
                            .decls
                            .iter()
                            .any(|d| matches!(d, Decl::Fn(f) if f.name == *name));
                        if is_user_fn {
                            // Evaluate args eagerly (left-to-right order preserved).
                            let mut arg_vals = Vec::with_capacity(args.len());
                            let mut prov_parents = Vec::with_capacity(args.len());
                            for a in args {
                                let v = self.eval(&a.value, env)?;
                                prov_parents.push(v.prov().head);
                                arg_vals.push(v);
                            }
                            return Ok(TailStep::TailCall {
                                fn_name: name.clone(),
                                args: arg_vals,
                                prov_parents,
                                span: *span,
                            });
                        }
                    }
                }
                // Not TCO-able: evaluate normally.
                Ok(TailStep::Done(self.eval(expr, env)?))
            }

            // Everything else: fall through to normal eval. ──────────────────
            _ => Ok(TailStep::Done(self.eval(expr, env)?)),
        }
    }

    pub fn eval(&mut self, e: &Expr, env: &mut Env) -> EResult<Value> {
        match e {
            Expr::Lit(l, s) => Ok(Value::from_lit(l, *s, &self.arena)),
            Expr::Var(name, s) => {
                if let Some(v) = env.lookup(name) {
                    return Ok(v.clone());
                }
                let let_decl = self.module.decls.iter().find_map(|d| {
                    if let Decl::Let(l) = d {
                        if l.name == *name {
                            Some(l.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });
                if let Some(l) = let_decl {
                    return self.eval(&l.value, env);
                }
                if self
                    .module
                    .decls
                    .iter()
                    .any(|d| matches!(d, Decl::Fn(f) if f.name == *name))
                {
                    let prov =
                        ProvChain::singleton(self.arena.clone(), ProvOp::Var(name.clone()), *s);
                    return Ok(Value::Fn(name.clone(), prov));
                }
                Err(EvalError::Unbound(name.clone()))
            }
            Expr::Bin(op, l, r, s) => {
                let lv = self.eval(l, env)?;
                let rv = self.eval(r, env)?;
                let result = eval_bin(*op, &lv, &rv)?;
                let prov = ProvChain::extend(
                    self.arena.clone(),
                    ProvOp::BinOp(op.as_str().into()),
                    *s,
                    vec![lv.prov().head, rv.prov().head],
                );
                Ok(result.with_prov(prov))
            }
            Expr::Un(op, x, s) => {
                let xv = self.eval(x, env)?;
                let result = eval_un(*op, &xv)?;
                let prov = ProvChain::extend(
                    self.arena.clone(),
                    ProvOp::UnOp(op.as_str().into()),
                    *s,
                    vec![xv.prov().head],
                );
                Ok(result.with_prov(prov))
            }
            Expr::Call { callee, args, span } => self.eval_call(callee, args, *span, env),
            Expr::If {
                cond,
                then_branch,
                else_branch,
                ..
            } => {
                let cv = self.eval(cond, env)?;
                let cb = cv
                    .as_bool()
                    .ok_or_else(|| EvalError::TypeError("if condition must be Bool".into()))?;
                if cb {
                    self.eval(then_branch, env)
                } else {
                    self.eval(else_branch, env)
                }
            }
            Expr::Block { stmts, tail, span } => {
                let mut inner = env.child();
                for st in stmts {
                    match st {
                        Stmt::Let {
                            pat: Pattern::Var(name, _),
                            value,
                            ..
                        } => {
                            let v = self.eval(value, &mut inner)?;
                            inner.bind(name.clone(), v);
                        }
                        Stmt::Let { value, .. } => {
                            self.eval(value, &mut inner)?;
                        }
                        Stmt::Expr(e) => {
                            self.eval(e, &mut inner)?;
                        }
                    }
                }
                if let Some(t) = tail {
                    self.eval(t, &mut inner)
                } else {
                    Ok(Value::unit(self.arena.clone(), *span))
                }
            }
            Expr::Let {
                pat: Pattern::Var(name, _),
                value,
                body,
                ..
            } => {
                let v = self.eval(value, env)?;
                let mut inner = env.child();
                inner.bind(name.clone(), v);
                self.eval(body, &mut inner)
            }
            Expr::Let { value, body, .. } => {
                self.eval(value, env)?;
                self.eval(body, env)
            }
            Expr::Tuple(elts, s) => {
                let mut vs = Vec::with_capacity(elts.len());
                let mut parents = Vec::with_capacity(elts.len());
                for e in elts {
                    let v = self.eval(e, env)?;
                    parents.push(v.prov().head);
                    vs.push(v);
                }
                let prov = ProvChain::extend(
                    self.arena.clone(),
                    ProvOp::Synthetic("tuple".into()),
                    *s,
                    parents,
                );
                Ok(Value::Tuple(vs, prov))
            }
            Expr::List(elts, s) => {
                let mut vs = Vec::with_capacity(elts.len());
                let mut parents = Vec::with_capacity(elts.len());
                for e in elts {
                    let v = self.eval(e, env)?;
                    parents.push(v.prov().head);
                    vs.push(v);
                }
                let prov = ProvChain::extend(
                    self.arena.clone(),
                    ProvOp::Synthetic("list".into()),
                    *s,
                    parents,
                );
                Ok(Value::List(vs, prov))
            }
            Expr::Record(fields, s) => {
                let mut out = Vec::with_capacity(fields.len());
                let mut parents = Vec::new();
                for (n, e) in fields {
                    let v = self.eval(e, env)?;
                    parents.push(v.prov().head);
                    out.push((n.clone(), v));
                }
                let prov = ProvChain::extend(
                    self.arena.clone(),
                    ProvOp::Synthetic("record".into()),
                    *s,
                    parents,
                );
                Ok(Value::Record(out, prov))
            }
            Expr::Field(e, name, s) => {
                let v = self.eval(e, env)?;
                match v {
                    Value::Record(fields, _) => fields
                        .into_iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, v)| v)
                        .ok_or_else(|| EvalError::TypeError(format!("no field `{name}`"))),
                    _ => Err(EvalError::TypeError(format!(
                        "field access at {s:?} on non-record"
                    ))),
                }
            }
            Expr::Index(e, i, _) => {
                let v = self.eval(e, env)?;
                let iv = self.eval(i, env)?;
                let idx = iv
                    .as_int()
                    .ok_or_else(|| EvalError::TypeError("index must be Int".into()))?
                    as usize;
                match v {
                    Value::List(items, _) => items
                        .get(idx)
                        .cloned()
                        .ok_or_else(|| EvalError::TypeError("index out of range".into())),
                    _ => Err(EvalError::TypeError("indexing non-list".into())),
                }
            }
            Expr::Confident { value, p, span } => {
                let vv = self.eval(value, env)?;
                let pv = self.eval(p, env)?;
                let pf = pv
                    .as_float()
                    .ok_or_else(|| EvalError::TypeError("confidence must be a number".into()))?;
                let prov = ProvChain::extend(
                    self.arena.clone(),
                    ProvOp::Confident(pf),
                    *span,
                    vec![vv.prov().head, pv.prov().head],
                );
                Ok(Value::Confident {
                    value: Box::new(vv),
                    p: pf,
                    prov,
                })
            }
            Expr::Assume(p, s) => {
                let pv = self.eval(p, env)?;
                if pv.as_bool() != Some(true) {
                    return Err(EvalError::User(
                        "assume predicate evaluated to false at runtime".into(),
                    ));
                }
                self.assumed.push((**p).clone());
                Ok(Value::unit_with_prov(ProvChain::singleton(
                    self.arena.clone(),
                    ProvOp::Assume,
                    *s,
                )))
            }
            Expr::Annot { expr, .. } => self.eval(expr, env),
            Expr::StrInterp { parts, span } => {
                let mut out = String::new();
                let mut prov_parents = Vec::new();
                for p in parts {
                    match p {
                        StrPart::Lit(s) => out.push_str(s),
                        StrPart::Expr(e) => {
                            let v = self.eval(e, env)?;
                            prov_parents.push(v.prov().head);
                            out.push_str(&v.display());
                        }
                    }
                }
                let prov = ProvChain::extend(
                    self.arena.clone(),
                    ProvOp::Synthetic("strinterp".into()),
                    *span,
                    prov_parents,
                );
                Ok(Value::Str(out, prov))
            }
            Expr::Lambda {
                params, body, span, ..
            } => {
                let prov = ProvChain::singleton(
                    self.arena.clone(),
                    ProvOp::Synthetic("lambda".into()),
                    *span,
                );
                Ok(Value::Closure {
                    params: params.clone(),
                    body: body.clone(),
                    env: env.clone(),
                    prov,
                })
            }
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => self.eval_match(scrutinee, arms, *span, env),
        }
    }

    fn eval_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
        env: &mut Env,
    ) -> EResult<Value> {
        let scrut = self.eval(scrutinee, env)?;
        for arm in arms {
            let mut local = env.child();
            if pattern_match(&arm.pat, &scrut, &mut local) {
                if let Some(guard) = &arm.guard {
                    let gv = self.eval(guard, &mut local)?;
                    if gv.as_bool() != Some(true) {
                        continue;
                    }
                }
                let v = self.eval(&arm.body, &mut local)?;
                let prov = ProvChain::extend(
                    self.arena.clone(),
                    ProvOp::Synthetic("match".into()),
                    span,
                    vec![scrut.prov().head, v.prov().head],
                );
                return Ok(v.with_prov(prov));
            }
        }
        Err(EvalError::User(format!(
            "non-exhaustive match at {span:?}: no arm matched value of type {}",
            scrut.type_name()
        )))
    }

    fn eval_call(
        &mut self,
        callee: &Expr,
        args: &[Arg],
        span: Span,
        env: &mut Env,
    ) -> EResult<Value> {
        let mut arg_vals = Vec::with_capacity(args.len());
        let mut prov_parents = Vec::with_capacity(args.len());
        for a in args {
            let v = self.eval(&a.value, env)?;
            prov_parents.push(v.prov().head);
            arg_vals.push(v);
        }

        // Direct identifier call: builtins and module-level fns win even if the name
        // is shadowed by an env binding (Aether shadowing is per-block; closures
        // explicitly bind into env, see below).
        if let Expr::Var(name, _) = callee {
            // 1) Shadowed by a closure-bearing binding in scope?
            if let Some(Value::Closure { .. }) = env.lookup(name) {
                let v = env.lookup(name).unwrap().clone();
                return self.apply_closure(v, arg_vals, prov_parents, name, span);
            }
            // 2) Built-in dispatch.
            if let Some(v) = builtins::dispatch(self, name, &arg_vals, span)? {
                return Ok(v);
            }
            // 3) ADT constructor: `type Shape = Circle(Float) | ...`
            {
                let is_ctor = self.module.decls.iter().any(|d| {
                    if let Decl::TypeAlias(ta) = d {
                        if let Type::Adt { ctors, .. } = &ta.ty {
                            return ctors.iter().any(|(cn, _)| cn == name);
                        }
                    }
                    false
                });
                if is_ctor {
                    let prov = ProvChain::extend(
                        self.arena.clone(),
                        ProvOp::Call(name.clone()),
                        span,
                        prov_parents,
                    );
                    return Ok(Value::Ctor {
                        name: name.clone(),
                        args: arg_vals,
                        prov,
                    });
                }
            }
            // 4) User-defined fn declaration.
            if let Some(f) = self.module.decls.iter().find_map(|d| {
                if let Decl::Fn(f) = d {
                    if f.name == *name {
                        Some(f.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }) {
                if f.params.len() != arg_vals.len() {
                    return Err(EvalError::TypeError(format!(
                        "{name}: expected {} args, got {}",
                        f.params.len(),
                        arg_vals.len()
                    )));
                }
                let result = self.eval_fn(&f, arg_vals, env)?;
                let prov = ProvChain::extend(
                    self.arena.clone(),
                    ProvOp::Call(name.clone()),
                    span,
                    prov_parents,
                );
                return Ok(result.with_prov(prov));
            }
            // 4) Maybe it's a `Value::Fn` (function reference) in env.
            if let Some(Value::Fn(ref n, _)) = env.lookup(name) {
                let n = n.clone();
                if let Some(v) = builtins::dispatch(self, &n, &arg_vals, span)? {
                    return Ok(v);
                }
                if let Some(f) = self.module.decls.iter().find_map(|d| {
                    if let Decl::Fn(f) = d {
                        if f.name == n {
                            Some(f.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }) {
                    let result = self.eval_fn(&f, arg_vals, env)?;
                    let prov =
                        ProvChain::extend(self.arena.clone(), ProvOp::Call(n), span, prov_parents);
                    return Ok(result.with_prov(prov));
                }
            }
            return Err(EvalError::UndefinedFn(name.clone()));
        }

        // Expression callee: must evaluate to a callable Value.
        let cv = self.eval(callee, env)?;
        match cv {
            Value::Closure { .. } => {
                self.apply_closure(cv, arg_vals, prov_parents, "<closure>", span)
            }
            Value::Fn(name, _) => {
                if let Some(v) = builtins::dispatch(self, &name, &arg_vals, span)? {
                    return Ok(v);
                }
                let f = self
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
                    .ok_or_else(|| EvalError::UndefinedFn(name.clone()))?;
                let result = self.eval_fn(&f, arg_vals, env)?;
                let prov =
                    ProvChain::extend(self.arena.clone(), ProvOp::Call(name), span, prov_parents);
                Ok(result.with_prov(prov))
            }
            other => Err(EvalError::TypeError(format!(
                "callee evaluates to {}, which is not callable",
                other.type_name()
            ))),
        }
    }

    fn apply_closure(
        &mut self,
        closure: Value,
        args: Vec<Value>,
        prov_parents: Vec<usize>,
        name: &str,
        span: Span,
    ) -> EResult<Value> {
        let Value::Closure {
            params,
            body,
            env: captured,
            ..
        } = closure
        else {
            return Err(EvalError::TypeError("apply_closure: not a closure".into()));
        };
        if params.len() != args.len() {
            return Err(EvalError::TypeError(format!(
                "closure: expected {} args, got {}",
                params.len(),
                args.len()
            )));
        }
        let mut local = captured.child();
        for (p, v) in params.iter().zip(args.into_iter()) {
            local.bind(p.name.clone(), v);
        }
        let result = self.eval(&body, &mut local)?;
        let prov = ProvChain::extend(
            self.arena.clone(),
            ProvOp::Call(name.into()),
            span,
            prov_parents,
        );
        Ok(result.with_prov(prov))
    }
}

/// Try to bind a `Pattern` against a `Value`, mutating `env` with any captures.
/// Returns `true` on a match.
fn pattern_match(pat: &Pattern, v: &Value, env: &mut Env) -> bool {
    match (pat, v) {
        (Pattern::Wild(_), _) => true,
        (Pattern::Var(name, _), _) => {
            env.bind(name.clone(), v.clone());
            true
        }
        (Pattern::Lit(lp, _), val) => match (lp, val) {
            (Lit::Int(n), Value::Int(m, _)) => n == m,
            (Lit::Float(a), Value::Float(b, _)) => a == b,
            (Lit::Bool(a), Value::Bool(b, _)) => a == b,
            (Lit::Str(a), Value::Str(b, _)) => a == b,
            (Lit::Unit, Value::Unit(_)) => true,
            _ => false,
        },
        (Pattern::Tuple(pats, _), Value::Tuple(vs, _)) => {
            if pats.len() != vs.len() {
                return false;
            }
            for (p, v) in pats.iter().zip(vs.iter()) {
                if !pattern_match(p, v, env) {
                    return false;
                }
            }
            true
        }
        (Pattern::Record(pats, _), Value::Record(fields, _)) => {
            for (n, p) in pats {
                let Some((_, v)) = fields.iter().find(|(fname, _)| fname == n) else {
                    return false;
                };
                if !pattern_match(p, v, env) {
                    return false;
                }
            }
            true
        }
        (
            Pattern::Ctor {
                name: pname,
                args: pargs,
                ..
            },
            Value::Ctor {
                name: vname,
                args: vargs,
                ..
            },
        ) => {
            if pname != vname || pargs.len() != vargs.len() {
                return false;
            }
            for (p, v) in pargs.iter().zip(vargs.iter()) {
                if !pattern_match(p, v, env) {
                    return false;
                }
            }
            true
        }
        (Pattern::Ctor { .. }, _) => false,
        _ => false,
    }
}

fn eval_bin(op: BinOp, l: &Value, r: &Value) -> EResult<Value> {
    use Value::*;
    match (op, l, r) {
        (BinOp::Add, Int(a, _), Int(b, _)) => Ok(Int(a + b, fake_prov())),
        (BinOp::Sub, Int(a, _), Int(b, _)) => Ok(Int(a - b, fake_prov())),
        (BinOp::Mul, Int(a, _), Int(b, _)) => Ok(Int(a * b, fake_prov())),
        (BinOp::Div, Int(_, _), Int(0, _)) => Err(EvalError::DivByZero),
        (BinOp::Div, Int(a, _), Int(b, _)) => Ok(Int(a / b, fake_prov())),
        (BinOp::Mod, Int(_, _), Int(0, _)) => Err(EvalError::DivByZero),
        (BinOp::Mod, Int(a, _), Int(b, _)) => Ok(Int(a % b, fake_prov())),

        (BinOp::Add, Float(a, _), Float(b, _)) => Ok(Float(a + b, fake_prov())),
        (BinOp::Sub, Float(a, _), Float(b, _)) => Ok(Float(a - b, fake_prov())),
        (BinOp::Mul, Float(a, _), Float(b, _)) => Ok(Float(a * b, fake_prov())),
        (BinOp::Div, Float(a, _), Float(b, _)) => Ok(Float(a / b, fake_prov())),

        (BinOp::Eq, a, b) => Ok(Bool(a.eq_val(b), fake_prov())),
        (BinOp::Neq, a, b) => Ok(Bool(!a.eq_val(b), fake_prov())),
        (BinOp::Lt, a, b) => Ok(Bool(
            a.cmp_val(b).map(|o| o.is_lt()).unwrap_or(false),
            fake_prov(),
        )),
        (BinOp::Le, a, b) => Ok(Bool(
            a.cmp_val(b).map(|o| o.is_le()).unwrap_or(false),
            fake_prov(),
        )),
        (BinOp::Gt, a, b) => Ok(Bool(
            a.cmp_val(b).map(|o| o.is_gt()).unwrap_or(false),
            fake_prov(),
        )),
        (BinOp::Ge, a, b) => Ok(Bool(
            a.cmp_val(b).map(|o| o.is_ge()).unwrap_or(false),
            fake_prov(),
        )),

        (BinOp::And, Bool(a, _), Bool(b, _)) => Ok(Bool(*a && *b, fake_prov())),
        (BinOp::Or, Bool(a, _), Bool(b, _)) => Ok(Bool(*a || *b, fake_prov())),
        (BinOp::Implies, Bool(a, _), Bool(b, _)) => Ok(Bool(!*a || *b, fake_prov())),

        (BinOp::Concat, Str(a, _), Str(b, _)) => Ok(Str(format!("{a}{b}"), fake_prov())),
        (BinOp::Concat, List(a, _), List(b, _)) => {
            let mut out = a.clone();
            out.extend(b.iter().cloned());
            Ok(List(out, fake_prov()))
        }
        (op, l, r) => Err(EvalError::TypeError(format!(
            "binop {} not defined for {} and {}",
            op.as_str(),
            l.type_name(),
            r.type_name()
        ))),
    }
}

fn eval_un(op: UnOp, v: &Value) -> EResult<Value> {
    use Value::*;
    match (op, v) {
        (UnOp::Neg, Int(n, _)) => Ok(Int(-*n, fake_prov())),
        (UnOp::Neg, Float(f, _)) => Ok(Float(-*f, fake_prov())),
        (UnOp::Not, Bool(b, _)) => Ok(Bool(!*b, fake_prov())),
        (op, v) => Err(EvalError::TypeError(format!(
            "unop {} not defined for {}",
            op.as_str(),
            v.type_name()
        ))),
    }
}

// ── Snapshot types & helpers ────────────────────────────────────────────────

/// A single label/value mismatch from a snapshot comparison.
#[derive(Debug, Clone)]
pub struct SnapMismatch {
    pub label: String,
    pub expected: String,
    pub got: String,
}

/// Status of one `snap "..." { ... }` block.
#[derive(Debug, Clone)]
pub enum SnapStatus {
    /// Values matched the golden file.
    Pass,
    /// File was absent (or `--update` passed) — values were written/updated.
    Captured,
    /// One or more label values differed from the golden.
    Mismatch(Vec<SnapMismatch>),
    /// The snap block itself threw an error.
    Error(String),
}

/// Per-block report returned by `run_snapshots`.
#[derive(Debug, Clone)]
pub struct SnapReport {
    pub name: String,
    pub fn_name: String,
    pub status: SnapStatus,
}

impl SnapReport {
    pub fn passed(&self) -> bool {
        matches!(self.status, SnapStatus::Pass | SnapStatus::Captured)
    }
}

/// Compare golden entries against captured entries; return mismatches.
fn diff_entries(golden: &[(String, String)], captured: &[(String, String)]) -> Vec<SnapMismatch> {
    let mut mismatches = Vec::new();
    // Check all golden labels appear in captured with correct value.
    for (label, expected) in golden {
        match captured.iter().find(|(l, _)| l == label) {
            Some((_, got)) if got == expected => {}
            Some((_, got)) => mismatches.push(SnapMismatch {
                label: label.clone(),
                expected: expected.clone(),
                got: got.clone(),
            }),
            None => mismatches.push(SnapMismatch {
                label: label.clone(),
                expected: expected.clone(),
                got: "<missing>".to_string(),
            }),
        }
    }
    // Check for new labels not in golden.
    for (label, got) in captured {
        if !golden.iter().any(|(l, _)| l == label) {
            mismatches.push(SnapMismatch {
                label: label.clone(),
                expected: "<not in golden>".to_string(),
                got: got.clone(),
            });
        }
    }
    mismatches
}

/// Parse a `.snap` file into `section_name -> Vec<(label, value)>`.
///
/// Format:
/// ```text
/// [section_name]
/// label = "value"
/// label2 = "value2"
/// ```
fn load_snap_file(
    path: &std::path::Path,
) -> std::collections::BTreeMap<String, Vec<(String, String)>> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(src) = std::fs::read_to_string(path) else {
        return map;
    };
    let mut current_section: Option<String> = None;
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = Some(line[1..line.len() - 1].to_string());
            map.entry(current_section.clone().unwrap()).or_default();
        } else if let Some(section) = &current_section {
            // Parse `label = "value"` — value may contain any chars inside quotes.
            if let Some((label_part, val_part)) = line.split_once('=') {
                let label = label_part.trim().to_string();
                let val_trimmed = val_part.trim();
                let value = if val_trimmed.starts_with('"')
                    && val_trimmed.ends_with('"')
                    && val_trimmed.len() >= 2
                {
                    // Unescape \" and \\
                    val_trimmed[1..val_trimmed.len() - 1]
                        .replace("\\\"", "\"")
                        .replace("\\\\", "\\")
                } else {
                    val_trimmed.to_string()
                };
                map.entry(section.clone()).or_default().push((label, value));
            }
        }
    }
    map
}

/// Serialize all sections back to a `.snap` file.
fn write_snap_file(
    path: &std::path::Path,
    sections: &std::collections::BTreeMap<String, Vec<(String, String)>>,
) -> std::io::Result<()> {
    use std::fmt::Write as FmtWrite;
    let mut out = String::new();
    for (section, entries) in sections {
        let _ = writeln!(out, "[{section}]");
        for (label, value) in entries {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = writeln!(out, "{label} = \"{escaped}\"");
        }
        let _ = writeln!(out);
    }
    std::fs::write(path, out)
}

fn fake_prov() -> ProvChain {
    let arena = ProvArena::new();
    ProvChain::singleton(arena, ProvOp::Synthetic("placeholder".into()), Span::DUMMY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_parser::parse_module;

    fn run(src: &str) -> EResult<Value> {
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        rt.capture_only = true;
        rt.run_main()
    }

    #[test]
    fn hello() {
        let v = run(r#"fn main() -> Unit effects {IO} { print("hi") }"#).unwrap();
        assert!(matches!(v, Value::Unit(_)));
    }

    #[test]
    fn arith() {
        let m = parse_module(FileId(0), "fn f() -> Int effects {} { 3 + 4 * 5 }").unwrap();
        let mut rt = Runtime::new(m);
        let mut env = Env::root();
        let f = match &rt.module.decls[0] {
            Decl::Fn(f) => f.clone(),
            _ => panic!(),
        };
        let v = rt.eval_fn(&f, vec![], &mut env).unwrap();
        assert_eq!(v.as_int(), Some(23));
    }

    #[test]
    fn closure_captures_env() {
        let src = "fn main() -> Int effects {} {
            let k = 10;
            let add_k = fn(x: Int) -> Int => x + k;
            add_k(5)
        }";
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        let f = match &rt.module.decls[0] {
            Decl::Fn(f) => f.clone(),
            _ => panic!(),
        };
        let mut env = Env::root();
        let v = rt.eval_fn(&f, vec![], &mut env).unwrap();
        assert_eq!(v.as_int(), Some(15));
    }

    fn find_main(rt: &Runtime) -> FnDecl {
        rt.module
            .decls
            .iter()
            .find_map(|d| {
                if let Decl::Fn(f) = d {
                    if f.name == "main" {
                        Some(f.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap()
    }

    #[test]
    fn closure_shadows_user_fn() {
        let src = r#"fn double(x: Int) -> Int effects {} { x * 2 }
        fn main() -> Int effects {} {
            let double = fn(x: Int) -> Int => x + 100;
            double(5)
        }"#;
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        let f = find_main(&rt);
        let mut env = Env::root();
        let v = rt.eval_fn(&f, vec![], &mut env).unwrap();
        assert_eq!(v.as_int(), Some(105));
    }

    #[test]
    fn match_lit_arms() {
        let src = r#"fn label(n: Int) -> Str effects {} {
            match n with {
                0 => "zero",
                1 => "one",
                _ => "many"
            }
        }
        fn main() -> Str effects {} { label(1) }"#;
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        let f = find_main(&rt);
        let mut env = Env::root();
        let v = rt.eval_fn(&f, vec![], &mut env).unwrap();
        assert_eq!(v.as_str(), Some("one"));
    }

    #[test]
    fn match_with_guard() {
        let src = r#"fn cls(n: Int) -> Str effects {} {
            match n with {
                x if x > 0 => "pos",
                x if x < 0 => "neg",
                _ => "zero"
            }
        }
        fn main() -> Str effects {} { cls(-3) }"#;
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        let f = find_main(&rt);
        let mut env = Env::root();
        let v = rt.eval_fn(&f, vec![], &mut env).unwrap();
        assert_eq!(v.as_str(), Some("neg"));
    }

    #[test]
    fn test_framework_runs() {
        let src = r#"
            test "arithmetic" {
                assert_eq(1 + 1, 2)
            }
            test "comparison" {
                assert(3 < 5)
                assert(5 > 3)
            }
            test "deliberately failing" {
                assert(false)
            }
        "#;
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        rt.capture_only = true;
        let reports = rt.run_tests();
        assert_eq!(reports.len(), 3);
        assert!(reports[0].passed(), "{:?}", reports[0]);
        assert!(reports[1].passed(), "{:?}", reports[1]);
        assert!(!reports[2].passed(), "{:?}", reports[2]);
    }

    #[test]
    fn tool_registry_overrides_llm_stub() {
        let mut rt =
            Runtime::new(parse_module(FileId(0), "fn main() -> Str effects {} { \"x\" }").unwrap());
        rt.capture_only = true;
        rt.tools.register("llm_complete", |args| {
            let prompt = args.first().and_then(Value::as_str).unwrap_or("");
            let arena = ProvArena::new();
            Ok(Value::Str(
                format!("OVERRIDE:{prompt}"),
                ProvChain::singleton(arena, ProvOp::Tool("llm_complete".into()), Span::DUMMY),
            ))
        });
        let src = r#"fn main() -> Str effects {Net, Throw} { llm_complete("hi") }"#;
        rt.module = parse_module(FileId(0), src).unwrap();
        let v = rt.run_main().unwrap();
        assert_eq!(v.as_str(), Some("OVERRIDE:hi"));
    }

    #[test]
    fn provenance_records() {
        let m = parse_module(FileId(0), "fn f() -> Int effects {} { 1 + 2 }").unwrap();
        let mut rt = Runtime::new(m);
        let mut env = Env::root();
        let f = match &rt.module.decls[0] {
            Decl::Fn(f) => f.clone(),
            _ => panic!(),
        };
        let v = rt.eval_fn(&f, vec![], &mut env).unwrap();
        let chain = v.prov();
        let nodes = chain.nodes_topo();
        assert!(nodes
            .iter()
            .any(|(_, n)| matches!(&n.op, ProvOp::BinOp(s) if s == "+")));
    }

    // ── ADT eval tests ──────────────────────────────────────────────────────────

    #[test]
    fn adt_ctor_builds_value() {
        // `Circle(5.0)` should evaluate to Value::Ctor { name: "Circle", args: [Float(5.0)] }.
        let src = "type Shape = Circle(Float) | Square(Float)\n\
                   fn main() -> Shape effects {} { Circle(5.0) }";
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        let f = find_main(&rt);
        let mut env = Env::root();
        let v = rt.eval_fn(&f, vec![], &mut env).unwrap();
        match &v {
            Value::Ctor { name, args, .. } => {
                assert_eq!(name, "Circle");
                assert_eq!(args.len(), 1);
                assert!((args[0].as_float().unwrap() - 5.0).abs() < 1e-9);
            }
            other => panic!("expected Ctor, got {:?}", other),
        }
    }

    #[test]
    fn adt_pattern_match_correct_arm() {
        // Match on Circle(5.0) — should bind `r = 5.0` and return 5.0.
        let src = "type Shape = Circle(Float) | Square(Float)\n\
                   fn area(s: Shape) -> Float effects {} {\n\
                     match s with {\n\
                       Circle(r) => r,\n\
                       Square(x) => x * x\n\
                     }\n\
                   }\n\
                   fn main() -> Float effects {} { area(Circle(5.0)) }";
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        let f = find_main(&rt);
        let mut env = Env::root();
        let v = rt.eval_fn(&f, vec![], &mut env).unwrap();
        assert!((v.as_float().unwrap() - 5.0).abs() < 1e-9, "got {:?}", v);
    }

    #[test]
    fn adt_pattern_match_second_arm() {
        // Match on Square(4.0) — should pick Square arm and return 16.0.
        let src = "type Shape = Circle(Float) | Square(Float)\n\
                   fn area(s: Shape) -> Float effects {} {\n\
                     match s with {\n\
                       Circle(r) => r,\n\
                       Square(x) => x * x\n\
                     }\n\
                   }\n\
                   fn main() -> Float effects {} { area(Square(4.0)) }";
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        let f = find_main(&rt);
        let mut env = Env::root();
        let v = rt.eval_fn(&f, vec![], &mut env).unwrap();
        assert!((v.as_float().unwrap() - 16.0).abs() < 1e-9, "got {:?}", v);
    }

    // ── TCO tests ───────────────────────────────────────────────────────────────

    /// 200_000-deep tail recursion must complete without stack overflow and
    /// return the correct value (0).
    #[test]
    fn deep_tail_recursion_no_overflow() {
        let src = r#"
            fn countdown(i: Int, n: Int) -> Int effects {} {
                if i >= n then 0 else countdown(i + 1, n)
            }
            fn main() -> Int effects {} { countdown(0, 200000) }
        "#;
        let v = run(src).unwrap();
        assert_eq!(v.as_int(), Some(0), "expected 0, got {:?}", v);
    }

    /// Non-tail (fib-style) recursion must still produce the correct answer.
    /// TCO must not silently break non-tail calls.
    #[test]
    fn tco_preserves_result() {
        let src = r#"
            fn fib(n: Int) -> Int effects {} {
                if n <= 1 then n else fib(n - 1) + fib(n - 2)
            }
            fn main() -> Int effects {} { fib(10) }
        "#;
        let v = run(src).unwrap();
        assert_eq!(v.as_int(), Some(55), "fib(10) should be 55, got {:?}", v);
    }

    /// A tail call inside an `if` branch must be TCO'd.
    #[test]
    fn tco_through_if() {
        let src = r#"
            fn count(i: Int, n: Int) -> Int effects {} {
                if i >= n then i else count(i + 1, n)
            }
            fn main() -> Int effects {} { count(0, 50000) }
        "#;
        let v = run(src).unwrap();
        assert_eq!(v.as_int(), Some(50000), "expected 50000, got {:?}", v);
    }

    /// A tail call inside a `match` arm must be TCO'd.
    #[test]
    fn tco_through_match() {
        let src = r#"
            fn step(i: Int, n: Int) -> Int effects {} {
                match i >= n with {
                    true  => i,
                    false => step(i + 1, n)
                }
            }
            fn main() -> Int effects {} { step(0, 50000) }
        "#;
        let v = run(src).unwrap();
        assert_eq!(v.as_int(), Some(50000), "expected 50000, got {:?}", v);
    }

    /// Mutual recursion (a → b → a) is not TCO'd (different functions), but
    /// must still produce the correct answer for modest depth.
    #[test]
    fn mutual_recursion_still_works() {
        let src = r#"
            fn is_even(n: Int) -> Bool effects {} {
                if n == 0 then true else is_odd(n - 1)
            }
            fn is_odd(n: Int) -> Bool effects {} {
                if n == 0 then false else is_even(n - 1)
            }
            fn main() -> Bool effects {} { is_even(100) }
        "#;
        let v = run(src).unwrap();
        assert_eq!(
            v.as_bool(),
            Some(true),
            "is_even(100) should be true, got {:?}",
            v
        );
    }

    #[test]
    fn snapshot_capture_then_verify() {
        let src = r#"
            snap "math checks" {
                snap_expect("add", str(1 + 2))
                snap_expect("mul", str(3 * 4))
            }
            fn main() -> Unit effects {} { () }
        "#;
        let m = parse_module(FileId(0), src).unwrap();
        let mut rt = Runtime::new(m);
        rt.capture_only = true;

        let dir = tempfile::tempdir().expect("tempdir");
        let snap_ae = dir.path().join("test_snap.ae");
        std::fs::write(&snap_ae, src).unwrap();

        // First run: capture (no golden file exists yet).
        let reports = rt.run_snapshots(&snap_ae, false);
        assert_eq!(reports.len(), 1);
        assert!(
            reports[0].passed(),
            "first run should capture: {:?}",
            reports[0].status
        );
        assert!(matches!(reports[0].status, SnapStatus::Captured));

        // Golden file must now exist.
        let golden = snap_ae.with_extension("snap");
        assert!(golden.exists(), ".snap file should have been created");

        // Second run: verify — should pass because values match.
        let m2 = parse_module(FileId(0), src).unwrap();
        let mut rt2 = Runtime::new(m2);
        rt2.capture_only = true;
        let reports2 = rt2.run_snapshots(&snap_ae, false);
        assert_eq!(reports2.len(), 1);
        assert!(
            reports2[0].passed(),
            "second run should pass: {:?}",
            reports2[0].status
        );
        assert!(matches!(reports2[0].status, SnapStatus::Pass));
    }
}
