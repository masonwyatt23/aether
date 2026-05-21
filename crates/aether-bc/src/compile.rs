//! Compile an `aether_ast::Module` into a `Program` of bytecode functions.

use std::collections::{HashMap, HashSet};

use aether_ast::decl::Param;
use aether_ast::pat::Pattern;
use aether_ast::ty::Type;
use aether_ast::{BinOp, Decl, Expr, FnDecl, Lit, Module, Stmt, UnOp};

use crate::error::CompileError;
use crate::op::{BuiltinId, Op};
use crate::vm::{BytecodeFn, Constant, Program};

// ─── public entry point ───────────────────────────────────────────────────────

/// Compile an entire module into a `Program`.
///
/// All `fn` declarations in the module are compiled.  The first function named
/// `"main"` (or the first function if no `"main"` exists) is set as the entry
/// point index.
pub fn compile_module(m: &Module) -> Result<Program, CompileError> {
    // Collect all function declarations.
    let fn_decls: Vec<&FnDecl> = m
        .decls
        .iter()
        .filter_map(|d| if let Decl::Fn(f) = d { Some(f) } else { None })
        .collect();

    // Build a name → index map for call resolution.
    let fn_index: HashMap<String, u32> = fn_decls
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i as u32))
        .collect();

    // Collect ADT constructor names from TypeAlias decls.
    // Maps ctor_name -> (arity, type_name).
    let mut ctor_map: HashMap<String, u8> = HashMap::new();
    for d in &m.decls {
        if let Decl::TypeAlias(ta) = d {
            if let Type::Adt { ctors, .. } = &ta.ty {
                for (ctor_name, fields) in ctors {
                    ctor_map.insert(ctor_name.clone(), fields.len() as u8);
                }
            }
        }
    }

    // Shared constant pool (strings).
    let mut constants: Vec<Constant> = Vec::new();

    // The closure functions emitted during compilation of top-level fns are
    // appended here; we pre-size fns to hold the top-level ones then extend.
    let n_top = fn_decls.len();
    let mut fns: Vec<BytecodeFn> = Vec::with_capacity(n_top);

    // We need a two-pass approach: first compile all top-level fns (which may
    // emit inner closures into `closure_fns`), then append closure fns.
    let mut closure_fns: Vec<BytecodeFn> = Vec::new();

    for decl in &fn_decls {
        let bf = compile_fn(
            decl,
            &fn_index,
            &ctor_map,
            &mut constants,
            n_top,
            &mut closure_fns,
        )?;
        fns.push(bf);
    }
    fns.extend(closure_fns);

    // Find `main` entry point.
    let entry = fn_decls
        .iter()
        .position(|f| f.name == "main")
        .map(|i| i as u32)
        .unwrap_or(0);

    Ok(Program {
        fns,
        constants,
        entry,
    })
}

// ─── per-function compilation ─────────────────────────────────────────────────

fn compile_fn(
    decl: &FnDecl,
    fn_index: &HashMap<String, u32>,
    ctor_map: &HashMap<String, u8>,
    constants: &mut Vec<Constant>,
    n_top: usize,
    closure_fns: &mut Vec<BytecodeFn>,
) -> Result<BytecodeFn, CompileError> {
    let mut ctx = FnCtx::new(fn_index, ctor_map, constants, n_top, closure_fns);

    // Bind parameters as the first locals (indices 0..arity).
    for p in &decl.params {
        ctx.declare_local(p.name.clone());
    }

    compile_expr(&decl.body, &mut ctx)?;
    ctx.emit(Op::Ret);

    Ok(BytecodeFn {
        name: decl.name.clone(),
        arity: decl.params.len() as u8,
        n_locals: ctx.n_locals,
        code: ctx.code,
    })
}

// ─── compilation context ──────────────────────────────────────────────────────

struct FnCtx<'a> {
    code: Vec<Op>,
    /// Stack of scopes; each scope maps name → local slot index.
    scopes: Vec<HashMap<String, u16>>,
    n_locals: u16,
    fn_index: &'a HashMap<String, u32>,
    ctor_map: &'a HashMap<String, u8>,
    constants: &'a mut Vec<Constant>,
    /// Number of top-level fns (closures are appended after them).
    n_top: usize,
    /// Closure functions emitted so far (appended to the program's fn list).
    closure_fns: &'a mut Vec<BytecodeFn>,
}

impl<'a> FnCtx<'a> {
    fn new(
        fn_index: &'a HashMap<String, u32>,
        ctor_map: &'a HashMap<String, u8>,
        constants: &'a mut Vec<Constant>,
        n_top: usize,
        closure_fns: &'a mut Vec<BytecodeFn>,
    ) -> Self {
        Self {
            code: Vec::new(),
            scopes: vec![HashMap::new()],
            n_locals: 0,
            fn_index,
            ctor_map,
            constants,
            n_top,
            closure_fns,
        }
    }

    fn emit(&mut self, op: Op) {
        self.code.push(op);
    }

    /// Declare a new local in the current scope, return its slot index.
    fn declare_local(&mut self, name: String) -> u16 {
        let idx = self.n_locals;
        self.n_locals += 1;
        self.scopes.last_mut().unwrap().insert(name, idx);
        idx
    }

    /// Look up an already-declared local (searches innermost scope first).
    fn lookup_local(&self, name: &str) -> Option<u16> {
        for scope in self.scopes.iter().rev() {
            if let Some(&idx) = scope.get(name) {
                return Some(idx);
            }
        }
        None
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Intern a string constant, return its index.
    fn intern_str(&mut self, s: &str) -> u32 {
        if let Some(i) = self
            .constants
            .iter()
            .position(|c| matches!(c, Constant::Str(t) if t == s))
        {
            return i as u32;
        }
        let idx = self.constants.len() as u32;
        self.constants.push(Constant::Str(s.to_string()));
        idx
    }

    /// Current instruction pointer (next instruction index).
    fn ip(&self) -> usize {
        self.code.len()
    }

    /// Patch a previously emitted jump instruction.
    fn patch_jump(&mut self, placeholder: usize) {
        let target = self.ip() as i32;
        let src = placeholder as i32;
        let offset = target - src - 1;
        match &mut self.code[placeholder] {
            Op::JumpIfFalse(o) | Op::Jump(o) => *o = offset,
            _ => panic!("patch_jump: not a jump instruction"),
        }
    }

    /// Allocate a new closure function index (n_top + closure_fns.len()).
    fn next_closure_idx(&self) -> u32 {
        (self.n_top + self.closure_fns.len()) as u32
    }
}

// ─── free variable analysis ───────────────────────────────────────────────────

/// Collect all free variables in an expression, given the names already in
/// scope as `bound`.  Returns a deterministic ordered Vec for stable capture slots.
fn free_vars_expr(e: &Expr, bound: &HashSet<String>, out: &mut Vec<String>) {
    match e {
        Expr::Var(name, _) => {
            if !bound.contains(name.as_str()) {
                if !out.contains(name) {
                    out.push(name.clone());
                }
            }
        }
        Expr::Lit(_, _) | Expr::Assume(_, _) => {}
        Expr::Bin(_, l, r, _) => {
            free_vars_expr(l, bound, out);
            free_vars_expr(r, bound, out);
        }
        Expr::Un(_, x, _) => free_vars_expr(x, bound, out),
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            free_vars_expr(cond, bound, out);
            free_vars_expr(then_branch, bound, out);
            free_vars_expr(else_branch, bound, out);
        }
        Expr::Let {
            pat, value, body, ..
        } => {
            free_vars_expr(value, bound, out);
            let mut inner = bound.clone();
            collect_pat_bindings(pat, &mut inner);
            free_vars_expr(body, &inner, out);
        }
        Expr::Block { stmts, tail, .. } => {
            let mut inner = bound.clone();
            for stmt in stmts {
                match stmt {
                    Stmt::Let { pat, value, .. } => {
                        free_vars_expr(value, &inner, out);
                        collect_pat_bindings(pat, &mut inner);
                    }
                    Stmt::Expr(e) => free_vars_expr(e, &inner, out),
                }
            }
            if let Some(t) = tail {
                free_vars_expr(t, &inner, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            free_vars_expr(callee, bound, out);
            for a in args {
                free_vars_expr(&a.value, bound, out);
            }
        }
        Expr::Lambda { params, body, .. } => {
            let mut inner = bound.clone();
            for p in params {
                inner.insert(p.name.clone());
            }
            free_vars_expr(body, &inner, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            free_vars_expr(scrutinee, bound, out);
            for arm in arms {
                let mut inner = bound.clone();
                collect_pat_bindings(&arm.pat, &mut inner);
                if let Some(g) = &arm.guard {
                    free_vars_expr(g, &inner, out);
                }
                free_vars_expr(&arm.body, &inner, out);
            }
        }
        Expr::Record(fields, _) => {
            for (_, v) in fields {
                free_vars_expr(v, bound, out);
            }
        }
        Expr::Tuple(elts, _) | Expr::List(elts, _) => {
            for e in elts {
                free_vars_expr(e, bound, out);
            }
        }
        Expr::Field(e, _, _) => free_vars_expr(e, bound, out),
        Expr::Index(e, i, _) => {
            free_vars_expr(e, bound, out);
            free_vars_expr(i, bound, out);
        }
        Expr::Annot { expr, .. } => free_vars_expr(expr, bound, out),
        Expr::Confident { value, p, .. } => {
            free_vars_expr(value, bound, out);
            free_vars_expr(p, bound, out);
        }
        Expr::StrInterp { parts, .. } => {
            for part in parts {
                if let aether_ast::StrPart::Expr(e) = part {
                    free_vars_expr(e, bound, out);
                }
            }
        }
    }
}

fn collect_pat_bindings(pat: &Pattern, out: &mut HashSet<String>) {
    match pat {
        Pattern::Var(name, _) => {
            out.insert(name.clone());
        }
        Pattern::Wild(_) | Pattern::Lit(_, _) => {}
        Pattern::Tuple(pats, _) | Pattern::Ctor { args: pats, .. } => {
            for p in pats {
                collect_pat_bindings(p, out);
            }
        }
        Pattern::Record(fields, _) => {
            for (_, p) in fields {
                collect_pat_bindings(p, out);
            }
        }
    }
}

// ─── expression compilation ───────────────────────────────────────────────────

fn compile_expr(e: &Expr, ctx: &mut FnCtx<'_>) -> Result<(), CompileError> {
    match e {
        Expr::Lit(lit, _) => {
            compile_lit(lit, ctx);
            Ok(())
        }

        Expr::Var(name, _) => {
            if let Some(idx) = ctx.lookup_local(name) {
                ctx.emit(Op::LoadLocal(idx));
                return Ok(());
            }
            // Could be a top-level function — handled as first-class fn value via
            // MakeClosure with zero captures (allows passing fns as values).
            if let Some(&fn_idx) = ctx.fn_index.get(name.as_str()) {
                ctx.emit(Op::MakeClosure {
                    fn_idx,
                    captured: vec![],
                });
                return Ok(());
            }
            Err(CompileError::UnboundVar(name.clone()))
        }

        Expr::Bin(op, l, r, _) => {
            compile_expr(l, ctx)?;
            compile_expr(r, ctx)?;
            compile_binop(*op, ctx);
            Ok(())
        }

        Expr::Un(op, x, _) => {
            compile_expr(x, ctx)?;
            match op {
                UnOp::Neg => ctx.emit(Op::Neg),
                UnOp::Not => ctx.emit(Op::Not),
            }
            Ok(())
        }

        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            compile_expr(cond, ctx)?;
            let jump_false = ctx.ip();
            ctx.emit(Op::JumpIfFalse(0));

            compile_expr(then_branch, ctx)?;
            let jump_end = ctx.ip();
            ctx.emit(Op::Jump(0));

            let else_start = ctx.ip() as i32;
            let jf_src = jump_false as i32;
            let jf_off = else_start - jf_src - 1;
            match &mut ctx.code[jump_false] {
                Op::JumpIfFalse(o) => *o = jf_off,
                _ => unreachable!(),
            }

            compile_expr(else_branch, ctx)?;
            ctx.patch_jump(jump_end);
            Ok(())
        }

        Expr::Let {
            pat: Pattern::Var(name, _),
            value,
            body,
            ..
        } => {
            compile_expr(value, ctx)?;
            let idx = ctx.declare_local(name.clone());
            ctx.emit(Op::StoreLocal(idx));
            compile_expr(body, ctx)?;
            Ok(())
        }

        Expr::Let {
            pat, value, body, ..
        } => {
            compile_expr(value, ctx)?;
            // Bind the pattern's variables from the value on the stack.
            compile_pat_bind(pat, ctx)?;
            compile_expr(body, ctx)?;
            Ok(())
        }

        Expr::Block { stmts, tail, .. } => {
            ctx.push_scope();
            for stmt in stmts {
                compile_stmt(stmt, ctx)?;
            }
            if let Some(t) = tail {
                compile_expr(t, ctx)?;
            } else {
                ctx.emit(Op::PushUnit);
            }
            ctx.pop_scope();
            Ok(())
        }

        Expr::Call { callee, args, .. } => {
            let argc = args.len() as u8;

            if let Expr::Var(name, _) = callee.as_ref() {
                // 1) Built-in?
                if let Some(bid) = BuiltinId::from_name(name) {
                    for a in args {
                        compile_expr(&a.value, ctx)?;
                    }
                    ctx.emit(Op::CallBuiltin {
                        id: bid as u16,
                        argc,
                    });
                    return Ok(());
                }
                // 2) ADT constructor?
                if let Some(&ctor_argc) = ctx.ctor_map.get(name.as_str()) {
                    for a in args {
                        compile_expr(&a.value, ctx)?;
                    }
                    let name_idx = ctx.intern_str(name);
                    ctx.emit(Op::Ctor {
                        name_idx,
                        argc: ctor_argc,
                    });
                    return Ok(());
                }
                // 3) User-defined function?
                if let Some(&fn_idx) = ctx.fn_index.get(name.as_str()) {
                    for a in args {
                        compile_expr(&a.value, ctx)?;
                    }
                    ctx.emit(Op::Call { fn_idx, argc });
                    return Ok(());
                }
                // 4) Local variable holding a closure?
                if let Some(local_idx) = ctx.lookup_local(name) {
                    ctx.emit(Op::LoadLocal(local_idx));
                    for a in args {
                        compile_expr(&a.value, ctx)?;
                    }
                    ctx.emit(Op::CallClosure { argc });
                    return Ok(());
                }
                // 5) Fallback: emit CallBuiltinDyn so the VM can try the
                //    dispatcher (e.g. tree-walker trampoline for stdlib fns,
                //    assert_eq, http_get, etc.).  This avoids a compile-time
                //    UndefinedFn error at the cost of a runtime error if no
                //    dispatcher is registered.
                for a in args {
                    compile_expr(&a.value, ctx)?;
                }
                let name_idx = ctx.intern_str(name);
                ctx.emit(Op::CallBuiltinDyn { name_idx, argc });
                return Ok(());
            }

            // Expression callee — compile it (could be a closure value), then call.
            compile_expr(callee, ctx)?;
            for a in args {
                compile_expr(&a.value, ctx)?;
            }
            ctx.emit(Op::CallClosure { argc });
            Ok(())
        }

        Expr::Lambda { params, body, .. } => compile_lambda(params, body, ctx),

        Expr::Match {
            scrutinee, arms, ..
        } => {
            compile_expr(scrutinee, ctx)?;
            compile_match(arms, ctx)
        }

        Expr::Record(fields, _) => {
            let field_names: Vec<u32> = fields
                .iter()
                .map(|(name, _)| ctx.intern_str(name))
                .collect();
            for (_, v) in fields {
                compile_expr(v, ctx)?;
            }
            ctx.emit(Op::MakeRecord { field_names });
            Ok(())
        }

        Expr::Tuple(elts, _) => {
            for e in elts {
                compile_expr(e, ctx)?;
            }
            ctx.emit(Op::MakeTuple(elts.len() as u16));
            Ok(())
        }

        Expr::List(elts, _) => {
            for e in elts {
                compile_expr(e, ctx)?;
            }
            ctx.emit(Op::MakeList(elts.len() as u16));
            Ok(())
        }

        Expr::Field(obj, field_name, _) => {
            compile_expr(obj, ctx)?;
            let name_idx = ctx.intern_str(field_name);
            ctx.emit(Op::FieldGet(name_idx));
            Ok(())
        }

        Expr::Index(obj, idx, _) => {
            compile_expr(obj, ctx)?;
            compile_expr(idx, ctx)?;
            ctx.emit(Op::Index);
            Ok(())
        }

        Expr::Annot { expr, .. } => compile_expr(expr, ctx),

        Expr::Assume(pred, _) => {
            compile_expr(pred, ctx)?;
            ctx.emit(Op::Pop);
            ctx.emit(Op::PushUnit);
            Ok(())
        }

        Expr::Confident { value, p, .. } => {
            // Compile the inner value then the probability expression, then
            // emit MakeConfident (pops p then inner, pushes Value::Confident).
            // This makes .display() match the tree-walker's
            // "{value} ~confidence({p})" format.
            compile_expr(value, ctx)?;
            compile_expr(p, ctx)?;
            ctx.emit(Op::MakeConfident);
            Ok(())
        }

        Expr::StrInterp { parts, .. } => compile_str_interp(parts, ctx),
    }
}

// ─── string interpolation ─────────────────────────────────────────────────────

fn compile_str_interp(
    parts: &[aether_ast::StrPart],
    ctx: &mut FnCtx<'_>,
) -> Result<(), CompileError> {
    if parts.is_empty() {
        // Empty interpolation → empty string.
        let idx = ctx.intern_str("");
        ctx.emit(Op::PushStr(idx));
        return Ok(());
    }

    // Compile each part onto the stack, converting exprs to Str via ToStr.
    // Then fold with (n_parts - 1) Concat ops.
    for part in parts {
        match part {
            aether_ast::StrPart::Lit(s) => {
                let idx = ctx.intern_str(s);
                ctx.emit(Op::PushStr(idx));
            }
            aether_ast::StrPart::Expr(e) => {
                compile_expr(e, ctx)?;
                ctx.emit(Op::ToStr);
            }
        }
    }

    // Fold the stack: (n-1) Concat ops reduce n strings to 1.
    for _ in 0..parts.len() - 1 {
        ctx.emit(Op::Concat);
    }

    Ok(())
}

// ─── lambda / closure compilation ────────────────────────────────────────────

fn compile_lambda(params: &[Param], body: &Expr, ctx: &mut FnCtx<'_>) -> Result<(), CompileError> {
    // Determine free variables in the lambda body that are in the enclosing scope.
    let mut bound: HashSet<String> = HashSet::new();
    for p in params {
        bound.insert(p.name.clone());
    }
    // Also consider all known top-level fn names as "globally bound" (not captured).
    for name in ctx.fn_index.keys() {
        bound.insert(name.clone());
    }
    for name in ctx.ctor_map.keys() {
        bound.insert(name.clone());
    }
    // Built-ins are globally bound too.
    for name in &["print", "println", "str", "int", "len", "abs", "max", "min"] {
        bound.insert(name.to_string());
    }

    let mut free: Vec<String> = Vec::new();
    free_vars_expr(body, &bound, &mut free);

    // Find local slot indices for each free variable in the enclosing frame.
    let captured_slots: Vec<u16> = free
        .iter()
        .map(|name| {
            ctx.lookup_local(name)
                .ok_or_else(|| CompileError::UnboundVar(name.clone()))
        })
        .collect::<Result<_, _>>()?;

    // Allocate closure function index before compiling (in case of recursion).
    let closure_fn_idx = ctx.next_closure_idx();

    // Compile the closure body into a fresh BytecodeFn.
    // The closure function's locals layout:
    //   slots 0..params.len()                  → parameters
    //   slots params.len()..params.len()+free.len() → captured values
    let mut inner_ctx = FnCtx::new(
        ctx.fn_index,
        ctx.ctor_map,
        ctx.constants,
        ctx.n_top,
        ctx.closure_fns,
    );
    // Declare params.
    for p in params {
        inner_ctx.declare_local(p.name.clone());
    }
    // Declare captured names at their capture slots.
    for name in &free {
        inner_ctx.declare_local(name.clone());
    }

    compile_expr(body, &mut inner_ctx)?;
    inner_ctx.emit(Op::Ret);

    // Extract what we need before inner_ctx goes out of scope (releases the
    // borrow on ctx.closure_fns).
    let (closure_code, closure_n_locals) = {
        let code = inner_ctx.code.clone();
        let n = inner_ctx.n_locals;
        (code, n)
    };
    let _ = inner_ctx; // consume to end borrow of ctx.closure_fns

    let closure_bf = BytecodeFn {
        name: format!("<closure@{}>", closure_fn_idx),
        arity: params.len() as u8,
        n_locals: closure_n_locals,
        code: closure_code,
    };
    ctx.closure_fns.push(closure_bf);

    // Emit MakeClosure in the enclosing function.
    ctx.emit(Op::MakeClosure {
        fn_idx: closure_fn_idx,
        captured: captured_slots,
    });
    Ok(())
}

// ─── match compilation ────────────────────────────────────────────────────────

/// Compile a match expression.  The scrutinee is already on the stack when
/// we enter.  Each arm:
///   1. Test the pattern (Dup scrutinee first if needed for ctor check).
///   2. If test fails, jump to next arm.
///   3. Bind pattern variables as new locals.
///   4. Evaluate guard (if any); if guard fails, jump to next arm.
///   5. Pop scrutinee, evaluate body, jump past all remaining arms.
fn compile_match(arms: &[aether_ast::MatchArm], ctx: &mut FnCtx<'_>) -> Result<(), CompileError> {
    // We'll collect the "jump to end" placeholders from each arm's body to patch.
    let mut end_jumps: Vec<usize> = Vec::new();

    for (arm_idx, arm) in arms.iter().enumerate() {
        let is_last = arm_idx == arms.len() - 1;

        ctx.push_scope();

        // Compile the pattern test.  On failure we jump to `next_arm_ph`
        // which will be patched once we know where the next arm starts.
        let next_arm_ph = compile_pattern_test(&arm.pat, ctx, is_last)?;

        // Compile guard if present.
        let guard_ph = if let Some(guard) = &arm.guard {
            // Scrutinee is still on stack; compile guard (it can reference
            // the just-bound variables from the pattern).
            compile_expr(guard, ctx)?;
            let ph = ctx.ip();
            ctx.emit(Op::JumpIfFalse(0));
            Some(ph)
        } else {
            None
        };

        // Pop the scrutinee (we consumed the pattern test, now pop the value).
        ctx.emit(Op::Pop);

        // Compile the arm body.
        compile_expr(&arm.body, ctx)?;

        // Jump past the rest of the arms.
        let end_jmp = ctx.ip();
        ctx.emit(Op::Jump(0));
        end_jumps.push(end_jmp);

        ctx.pop_scope();

        // Patch next-arm jump.
        if let Some(ph) = next_arm_ph {
            let here = ctx.ip() as i32;
            let src = ph as i32;
            let off = here - src - 1;
            match &mut ctx.code[ph] {
                Op::JumpIfFalse(o) | Op::Jump(o) => *o = off,
                _ => panic!("compile_match: expected jump placeholder"),
            }
        }
        if let Some(ph) = guard_ph {
            ctx.patch_jump(ph);
        }
    }

    // If no arm matched at runtime we'd fall through here — push Unit as
    // a safe fallback (well-typed Aether should never reach this).
    ctx.emit(Op::Pop); // pop scrutinee in the fallthrough case
    ctx.emit(Op::PushUnit);

    // Patch all end-jumps to here.
    for ph in end_jumps {
        ctx.patch_jump(ph);
    }

    Ok(())
}

/// Compile the pattern *test* for one arm.
///
/// The scrutinee is assumed to be on the stack.
///
/// Returns `Some(placeholder)` for a JumpIfFalse (or MatchCtor's jump) that
/// must be patched to the start of the NEXT arm when the match fails, or
/// `None` if the pattern is a wildcard / var that always succeeds.
///
/// After a successful test:
/// - Ctor patterns: scrutinee remains on stack; bound variables are loaded.
/// - Lit patterns: scrutinee remains on stack.
/// - Var patterns: scrutinee remains on stack; the var local is set.
/// - Wild patterns: scrutinee remains on stack.
fn compile_pattern_test(
    pat: &Pattern,
    ctx: &mut FnCtx<'_>,
    _is_last: bool,
) -> Result<Option<usize>, CompileError> {
    match pat {
        Pattern::Wild(_) => Ok(None),

        Pattern::Var(name, _) => {
            // Dup scrutinee, store into new local; scrutinee stays on stack.
            ctx.emit(Op::Dup);
            let idx = ctx.declare_local(name.clone());
            ctx.emit(Op::StoreLocal(idx));
            Ok(None)
        }

        Pattern::Lit(lit, _) => {
            // Dup scrutinee, push literal, test equality.
            ctx.emit(Op::Dup);
            compile_lit(lit, ctx);
            ctx.emit(Op::Eq);
            let ph = ctx.ip();
            ctx.emit(Op::JumpIfFalse(0));
            Ok(Some(ph))
        }

        Pattern::Ctor {
            name,
            args: sub_pats,
            ..
        } => {
            let name_idx = ctx.intern_str(name);
            let expect_arity = sub_pats.len() as u8;

            // MatchCtor peeks at TOS, pushes a Bool result (does not pop scrutinee).
            ctx.emit(Op::MatchCtor {
                name_idx,
                expect_arity,
                jump_if_miss: 0,
            });

            // JumpIfFalse on the bool result — patched later to skip to next arm.
            let jif_ph = ctx.ip();
            ctx.emit(Op::JumpIfFalse(0));

            // Now extract sub-pattern bindings (scrutinee still on top of stack).
            for (i, sub_pat) in sub_pats.iter().enumerate() {
                compile_ctor_sub_pattern(sub_pat, i as u8, ctx)?;
            }

            Ok(Some(jif_ph))
        }

        Pattern::Tuple(sub_pats, _) => {
            // For tuple patterns, we test element-by-element.
            // Dup scrutinee, then index into it.
            let mut last_ph: Option<usize> = None;
            for (i, sub_pat) in sub_pats.iter().enumerate() {
                match sub_pat {
                    Pattern::Wild(_) => {} // no test needed
                    Pattern::Var(name, _) => {
                        ctx.emit(Op::Dup);
                        ctx.emit(Op::TupleGet(i as u16));
                        let idx = ctx.declare_local(name.clone());
                        ctx.emit(Op::StoreLocal(idx));
                    }
                    Pattern::Lit(lit, _) => {
                        ctx.emit(Op::Dup);
                        ctx.emit(Op::TupleGet(i as u16));
                        compile_lit(lit, ctx);
                        ctx.emit(Op::Eq);
                        let ph = ctx.ip();
                        ctx.emit(Op::JumpIfFalse(0));
                        last_ph = Some(ph);
                    }
                    _ => return Err(CompileError::Unsupported("nested tuple pattern".into())),
                }
            }
            Ok(last_ph)
        }

        Pattern::Record(fields, _) => {
            for (field_name, sub_pat) in fields {
                match sub_pat {
                    Pattern::Wild(_) => {}
                    Pattern::Var(var_name, _) => {
                        let name_idx = ctx.intern_str(field_name);
                        ctx.emit(Op::Dup);
                        ctx.emit(Op::FieldGet(name_idx));
                        let idx = ctx.declare_local(var_name.clone());
                        ctx.emit(Op::StoreLocal(idx));
                    }
                    _ => return Err(CompileError::Unsupported("nested record pattern".into())),
                }
            }
            Ok(None)
        }
    }
}

/// Extract one sub-pattern from a Ctor match. The ctor value is on TOS.
fn compile_ctor_sub_pattern(
    pat: &Pattern,
    field_idx: u8,
    ctx: &mut FnCtx<'_>,
) -> Result<(), CompileError> {
    match pat {
        Pattern::Wild(_) => {} // nothing to do
        Pattern::Var(name, _) => {
            ctx.emit(Op::CtorField(field_idx));
            let idx = ctx.declare_local(name.clone());
            ctx.emit(Op::StoreLocal(idx));
        }
        Pattern::Lit(_lit, _) => {
            // We'd need to test the field value; for now just skip.
            // Full nested literal matching in ctors would need more jump infra.
            return Err(CompileError::Unsupported(
                "literal pattern inside ctor pattern".into(),
            ));
        }
        _ => {
            return Err(CompileError::Unsupported(
                "nested pattern inside ctor pattern".into(),
            ))
        }
    }
    Ok(())
}

/// Compile a pattern binding for a `let` with a destructuring pattern.
/// The value is already on the stack; we bind variables and pop the value.
fn compile_pat_bind(pat: &Pattern, ctx: &mut FnCtx<'_>) -> Result<(), CompileError> {
    match pat {
        Pattern::Var(name, _) => {
            let idx = ctx.declare_local(name.clone());
            ctx.emit(Op::StoreLocal(idx));
        }
        Pattern::Wild(_) => {
            ctx.emit(Op::Pop);
        }
        Pattern::Tuple(sub_pats, _) => {
            // Store the tuple into a temp local, then extract fields.
            let tmp = ctx.declare_local(format!("__tup_{}", ctx.n_locals));
            ctx.emit(Op::StoreLocal(tmp));
            for (i, sub_pat) in sub_pats.iter().enumerate() {
                match sub_pat {
                    Pattern::Var(name, _) => {
                        ctx.emit(Op::LoadLocal(tmp));
                        ctx.emit(Op::TupleGet(i as u16));
                        let idx = ctx.declare_local(name.clone());
                        ctx.emit(Op::StoreLocal(idx));
                    }
                    Pattern::Wild(_) => {}
                    _ => return Err(CompileError::Unsupported("nested let tuple pattern".into())),
                }
            }
        }
        _ => {
            // Discard — best effort
            ctx.emit(Op::Pop);
        }
    }
    Ok(())
}

fn compile_stmt(stmt: &Stmt, ctx: &mut FnCtx<'_>) -> Result<(), CompileError> {
    match stmt {
        Stmt::Let {
            pat: Pattern::Var(name, _),
            value,
            ..
        } => {
            compile_expr(value, ctx)?;
            let idx = ctx.declare_local(name.clone());
            ctx.emit(Op::StoreLocal(idx));
            Ok(())
        }
        Stmt::Let { pat, value, .. } => {
            compile_expr(value, ctx)?;
            compile_pat_bind(pat, ctx)?;
            Ok(())
        }
        Stmt::Expr(e) => {
            compile_expr(e, ctx)?;
            ctx.emit(Op::Pop);
            Ok(())
        }
    }
}

fn compile_lit(lit: &Lit, ctx: &mut FnCtx<'_>) {
    match lit {
        Lit::Int(n) => ctx.emit(Op::PushInt(*n)),
        Lit::Bool(b) => ctx.emit(Op::PushBool(*b)),
        Lit::Str(s) => {
            let idx = ctx.intern_str(s);
            ctx.emit(Op::PushStr(idx));
        }
        Lit::Unit => ctx.emit(Op::PushUnit),
        Lit::Float(f) => ctx.emit(Op::PushFloat(*f)),
    }
}

fn compile_binop(op: BinOp, ctx: &mut FnCtx<'_>) {
    match op {
        BinOp::Add => ctx.emit(Op::Add),
        BinOp::Sub => ctx.emit(Op::Sub),
        BinOp::Mul => ctx.emit(Op::Mul),
        BinOp::Div => ctx.emit(Op::Div),
        BinOp::Mod => ctx.emit(Op::Mod),
        BinOp::Eq => ctx.emit(Op::Eq),
        BinOp::Neq => ctx.emit(Op::Neq),
        BinOp::Lt => ctx.emit(Op::Lt),
        BinOp::Le => ctx.emit(Op::Le),
        BinOp::Gt => ctx.emit(Op::Gt),
        BinOp::Ge => ctx.emit(Op::Ge),
        BinOp::And => ctx.emit(Op::And),
        BinOp::Or => ctx.emit(Op::Or),
        BinOp::Concat => ctx.emit(Op::Concat),
        BinOp::Implies => {
            ctx.emit(Op::Implies);
        }
    }
}
