//! Aether type, effect, and refinement checker.
//!
//! Pipeline:
//!   1. `collect_signatures`: walk module, build symbol table of fn / tool / let / type sigs.
//!   2. `check_decl`: bidirectional type-check each decl body against its signature.
//!   3. Effect propagation: every observed effect must be in the declared row.
//!   4. Refinement check: at fn boundaries, prove each `ensures` clause via `refine`.
//!
//! The solver in `refine` is a sound (but incomplete) Fourier-Motzkin–style
//! decision procedure for linear integer/rational arithmetic with boolean
//! combinations. It is deliberately conservative: on anything it can't decide
//! it reports `Verdict::Unknown` — the type-checker downgrades these to
//! "could not verify" diagnostics rather than rejections.

pub mod builtins;
pub mod check;
pub mod ctx;
pub mod refine;
pub mod smt;
pub mod suggest;

pub use check::{check_module, Diagnostic, Severity};
pub use ctx::{FnSig, TypeCtx};
pub use refine::Verdict;
