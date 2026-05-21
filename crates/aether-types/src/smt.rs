//! Optional SMT escalation for the refinement checker.
//!
//! The built-in solver ([`crate::refine::prove_linear`]) decides linear
//! integer arithmetic. Anything outside that fragment — most importantly
//! non-linear terms like `x * y` — it reports as `Unknown`. When that
//! happens, [`crate::refine::prove`] escalates the query here.
//!
//! This module translates the goal to SMT-LIB2 and shells out to a `z3`
//! binary if one is on `PATH`. It takes **no build dependency** on Z3: when
//! `z3` is not installed (or `AETHER_DISABLE_SMT` is set) every query returns
//! `Verdict::Unknown`, so behavior is identical to a system without it.
//!
//! ## Soundness
//!
//! To prove `H ⊢ G` we ask the solver whether `H ∧ ¬G` is satisfiable over
//! the integers:
//!
//! - `unsat`   → no counterexample exists → the implication holds → `Proved`.
//! - `sat`     → a counterexample exists → `RefutedWith` (model parsed best-effort).
//! - `unknown` / any error / solver absent → `Unknown` (never a false `Proved`).
//!
//! A wrong `Proved` would be a critical bug; every uncertain path here is
//! funneled to `Unknown`, which the type-checker downgrades to a warning.

use crate::refine::{subst, Verdict};
use aether_ast::{BinOp, Expr, Lit, UnOp};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Largest `forall_in` range we will unroll into a conjunction.
const MAX_UNROLL: i64 = 256;

/// Try to discharge `hypotheses ⊢ goal` with an external SMT solver.
/// Returns `Verdict::Unknown` when no solver is available or the query
/// cannot be translated — never a false `Proved`.
pub fn prove_smt(hypotheses: &[Expr], goal: &Expr) -> Verdict {
    if !smt_enabled() {
        return Verdict::Unknown;
    }
    let Some(script) = build_script(hypotheses, goal) else {
        return Verdict::Unknown;
    };
    match run_z3(&script) {
        Some(Z3Answer::Unsat) => Verdict::Proved,
        Some(Z3Answer::Sat(values)) => Verdict::RefutedWith { values },
        Some(Z3Answer::Unknown) | None => Verdict::Unknown,
    }
}

/// Whether SMT escalation should be attempted: a `z3` binary exists and the
/// `AETHER_DISABLE_SMT` escape hatch is not set. The probe runs once.
pub fn smt_enabled() -> bool {
    if std::env::var_os("AETHER_DISABLE_SMT").is_some() {
        return false;
    }
    z3_available()
}

/// `true` if a usable `z3` binary is on `PATH`. Cached after the first probe.
pub fn z3_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("z3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

// ── SMT-LIB2 script construction ─────────────────────────────────────────────

fn build_script(hypotheses: &[Expr], goal: &Expr) -> Option<String> {
    let mut vars: BTreeSet<String> = BTreeSet::new();
    let mut hyp_terms: Vec<String> = Vec::with_capacity(hypotheses.len());
    for h in hypotheses {
        hyp_terms.push(emit(h, &mut vars)?);
    }
    let goal_term = emit(goal, &mut vars)?;

    let mut s = String::new();
    // Quantifier-free non-linear integer arithmetic. `forall_in` is unrolled,
    // so the emitted script never contains a real quantifier.
    s.push_str("(set-logic QF_NIA)\n");
    for v in &vars {
        s.push_str(&format!("(declare-const {v} Int)\n"));
    }
    for h in &hyp_terms {
        s.push_str(&format!("(assert {h})\n"));
    }
    // Satisfiability of (H ∧ ¬G): a model is a counterexample to H ⊢ G.
    s.push_str(&format!("(assert (not {goal_term}))\n"));
    s.push_str("(check-sat)\n");
    s.push_str("(get-model)\n");
    Some(s)
}

/// Translate an Aether boolean/arithmetic expression to an SMT-LIB2 term,
/// recording every free variable it mentions. Returns `None` for anything
/// that cannot be faithfully translated.
fn emit(e: &Expr, vars: &mut BTreeSet<String>) -> Option<String> {
    match e {
        Expr::Lit(Lit::Int(n), _) => Some(smt_int(*n)),
        Expr::Lit(Lit::Bool(b), _) => Some(b.to_string()),
        // Float / Str / Unit literals are not part of the arithmetic fragment.
        Expr::Lit(_, _) => None,
        Expr::Var(name, _) => {
            vars.insert(name.clone());
            Some(name.clone())
        }
        Expr::Un(UnOp::Neg, inner, _) => {
            let t = emit(inner, vars)?;
            Some(format!("(- {t})"))
        }
        Expr::Un(UnOp::Not, inner, _) => {
            let t = emit(inner, vars)?;
            Some(format!("(not {t})"))
        }
        Expr::Bin(op, l, r, _) => {
            let lt = emit(l, vars)?;
            let rt = emit(r, vars)?;
            let smt_op = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "div",
                BinOp::Mod => "mod",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::Eq => "=",
                BinOp::And => "and",
                BinOp::Or => "or",
                BinOp::Implies => "=>",
                BinOp::Neq => return Some(format!("(not (= {lt} {rt}))")),
                // String concat is not arithmetic.
                BinOp::Concat => return None,
            };
            Some(format!("({smt_op} {lt} {rt})"))
        }
        // `forall_in(x, lo, hi, pred)` — unroll the bounded quantifier into a
        // conjunction, exactly as the linear solver does.
        Expr::Call { callee, args, .. } => {
            let Expr::Var(fname, _) = callee.as_ref() else {
                return None;
            };
            if fname != "forall_in" || args.len() != 4 {
                return None;
            }
            let Expr::Var(bound, _) = &args[0].value else {
                return None;
            };
            let lo = as_int_literal(&args[1].value)?;
            let hi = as_int_literal(&args[2].value)?;
            if hi < lo || hi - lo > MAX_UNROLL {
                return None;
            }
            let pred = &args[3].value;
            let mut conjuncts = Vec::new();
            for k in lo..=hi {
                let instance = subst(pred, bound, &Expr::Lit(Lit::Int(k), span_of(pred)));
                conjuncts.push(emit(&instance, vars)?);
            }
            if conjuncts.is_empty() {
                Some("true".to_string())
            } else {
                Some(format!("(and {})", conjuncts.join(" ")))
            }
        }
        _ => None,
    }
}

/// SMT-LIB2 has no negative numeral literal — it is `(- n)`.
fn smt_int(n: i64) -> String {
    if n < 0 {
        format!("(- {})", n.unsigned_abs())
    } else {
        n.to_string()
    }
}

fn as_int_literal(e: &Expr) -> Option<i64> {
    match e {
        Expr::Lit(Lit::Int(n), _) => Some(*n),
        Expr::Un(UnOp::Neg, inner, _) => as_int_literal(inner).map(|n| -n),
        _ => None,
    }
}

fn span_of(e: &Expr) -> aether_ast::Span {
    e.span()
}

// ── z3 subprocess ────────────────────────────────────────────────────────────

enum Z3Answer {
    Unsat,
    Sat(BTreeMap<String, i64>),
    Unknown,
}

fn run_z3(script: &str) -> Option<Z3Answer> {
    // `-T:5` caps each query at 5 seconds so a hard goal can't hang the
    // type-checker; `-smt2 -in` reads the script from stdin.
    let mut child = Command::new("z3")
        .args(["-smt2", "-in", "-T:5"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(script.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next().unwrap_or("").trim();
    match first {
        "unsat" => Some(Z3Answer::Unsat),
        "sat" => Some(Z3Answer::Sat(parse_model(&text))),
        _ => Some(Z3Answer::Unknown),
    }
}

/// Best-effort extraction of integer assignments from a z3 `(get-model)`
/// block. Lines look like `(define-fun x () Int 5)` or, for negatives,
/// `(define-fun x () Int (- 5))`. Anything unparseable is simply skipped —
/// the witness is a UX nicety, not load-bearing for soundness.
fn parse_model(text: &str) -> BTreeMap<String, i64> {
    let mut model = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("(define-fun ") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(name) = parts.next() else { continue };
        // Expect `() Int <value...>`
        if parts.next() != Some("()") || parts.next() != Some("Int") {
            continue;
        }
        let value_text: String = parts.collect::<Vec<_>>().join(" ");
        let cleaned = value_text.trim_end_matches(')').trim();
        let parsed = if let Some(neg) = cleaned.strip_prefix("(- ") {
            neg.trim().parse::<i64>().ok().map(|n| -n)
        } else {
            cleaned.parse::<i64>().ok()
        };
        if let Some(v) = parsed {
            model.insert(name.to_string(), v);
        }
    }
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::FileId;
    use aether_parser::parse_expr;

    fn e(s: &str) -> Expr {
        parse_expr(FileId(0), s).unwrap()
    }

    /// Skip a test body when no z3 binary is installed — keeps `cargo test`
    /// green on machines without one while still exercising the integration
    /// wherever z3 is present (the CI `smt` job installs it).
    fn require_z3() -> bool {
        if z3_available() {
            true
        } else {
            eprintln!("skipping SMT test: no `z3` on PATH");
            false
        }
    }

    #[test]
    fn emits_nonlinear_term() {
        let mut vars = BTreeSet::new();
        let smt = emit(&e("x * x >= 0"), &mut vars).expect("should translate");
        assert!(smt.contains('*'), "expected a multiplication: {smt}");
        assert!(vars.contains("x"));
    }

    #[test]
    fn rejects_string_concat() {
        let mut vars = BTreeSet::new();
        assert!(emit(&e("a ++ b"), &mut vars).is_none());
    }

    #[test]
    fn smt_int_handles_negatives() {
        assert_eq!(smt_int(5), "5");
        assert_eq!(smt_int(-5), "(- 5)");
    }

    #[test]
    fn parse_model_reads_assignments() {
        let text = "sat\n(\n  (define-fun x () Int 7)\n  (define-fun y () Int (- 3))\n)";
        let m = parse_model(text);
        assert_eq!(m.get("x"), Some(&7));
        assert_eq!(m.get("y"), Some(&-3));
    }

    #[test]
    fn proves_nonlinear_square_nonnegative() {
        // `x * x >= 0` is true for every integer but outside the linear
        // fragment — the built-in solver returns Unknown; z3 proves it.
        if !require_z3() {
            return;
        }
        let v = prove_smt(&[], &e("x * x >= 0"));
        assert_eq!(v, Verdict::Proved, "z3 should prove x*x >= 0");
    }

    #[test]
    fn refutes_false_nonlinear_goal() {
        // `x * x == 2` has no integer solution near 0 but the goal
        // `x * x >= 5` is false for x = 1 — z3 must report a counterexample.
        if !require_z3() {
            return;
        }
        let v = prove_smt(&[], &e("x * x >= 5"));
        assert!(v.is_refuted(), "expected a counterexample, got {v:?}");
    }

    #[test]
    fn nonlinear_with_hypothesis() {
        // Given `x >= 2`, the goal `x * x >= x` holds for all integers.
        if !require_z3() {
            return;
        }
        let v = prove_smt(&[e("x >= 2")], &e("x * x >= x"));
        assert_eq!(v, Verdict::Proved);
    }

    #[test]
    fn disable_env_forces_unknown() {
        // The escape hatch must always win, regardless of z3 availability.
        std::env::set_var("AETHER_DISABLE_SMT", "1");
        let v = prove_smt(&[], &e("x * x >= 0"));
        std::env::remove_var("AETHER_DISABLE_SMT");
        assert_eq!(v, Verdict::Unknown);
    }
}
