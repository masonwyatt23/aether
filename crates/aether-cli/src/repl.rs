//! Interactive REPL for Aether.
//!
//! The `Engine` struct drives the REPL loop and is testable independently of
//! stdin/stdout. The binary entry-point in `main.rs` wires a `rustyline` editor
//! to the engine.

use aether_ast::{Decl, FileId, Module, SourceMap, Span};
use aether_eval::Runtime;
use aether_parser::{parse_expr, parse_module};
use aether_types::{check_module, Severity};

/// Result of feeding one line to the REPL engine.
#[derive(Debug)]
pub enum ReplOutput {
    /// User typed `:q` — the caller should exit the loop.
    Quit,
    /// Blank line or whitespace only — nothing to do.
    Empty,
    /// A value was produced; display it.
    Value(String),
    /// A declaration was added to the session.
    Defined(String),
    /// Type of an expression.
    Type(String),
    /// Debug (AST) of an expression.
    Debug(String),
    /// Provenance chain of an evaluated expression.
    Prov(String),
    /// Help text.
    Help(String),
    /// A file was loaded.
    Loaded(String),
    /// An error occurred; continue the loop.
    Error(String),
}

const HELP: &str = "\
Aether REPL commands:
  :q            quit
  :t <expr>     show the type of an expression
  :d <expr>     show the AST debug repr of an expression
  :p <expr>     show the provenance chain of an evaluated expression
  :l <file.ae>  load declarations from a file into this session
  :h            show this help

Any other input is parsed first as an expression (evaluated and printed), then
as a top-level declaration (added to the in-memory module).";

/// The persistent session state, decoupled from I/O.
pub struct Engine {
    /// Accumulates all declarations seen so far.
    module: Module,
    /// Source map — we add a synthetic entry per input line.
    sm: SourceMap,
    /// Counter for synthetic file ids.
    seq: u32,
}

impl Engine {
    pub fn new() -> Self {
        let sm = SourceMap::new();
        let fid = FileId(0);
        let module = Module {
            name: Some("repl".into()),
            doc: None,
            decls: vec![],
            span: Span::new(fid, 0..0),
        };
        Self { module, sm, seq: 1 }
    }

    fn next_fid(&mut self) -> FileId {
        let fid = FileId(self.seq);
        self.seq += 1;
        fid
    }

    /// Process one line of input, returning the appropriate output.
    pub fn feed(&mut self, line: &str) -> ReplOutput {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return ReplOutput::Empty;
        }

        // --- special commands ---
        if trimmed == ":q" {
            return ReplOutput::Quit;
        }
        if trimmed == ":h" || trimmed == ":help" {
            return ReplOutput::Help(HELP.to_string());
        }
        if let Some(rest) = trimmed.strip_prefix(":t ") {
            return self.cmd_type(rest.trim());
        }
        if let Some(rest) = trimmed.strip_prefix(":d ") {
            return self.cmd_debug(rest.trim());
        }
        if let Some(rest) = trimmed.strip_prefix(":p ") {
            return self.cmd_prov(rest.trim());
        }
        if let Some(rest) = trimmed.strip_prefix(":l ") {
            return self.cmd_load(rest.trim());
        }
        // Unknown colon command.
        if trimmed.starts_with(':') {
            return ReplOutput::Error(format!("unknown command `{trimmed}`; type :h for help"));
        }

        // --- try as expression first ---
        let fid = self.next_fid();
        self.sm
            .add(format!("<repl:{}>", fid.0), trimmed.to_string());
        if let Ok(expr) = parse_expr(fid, trimmed) {
            // Build a temporary module that merges session decls with the expression.
            let mut rt = Runtime::new(self.module.clone());
            rt.capture_only = true;
            match rt.eval_root(&expr) {
                Ok(v) => return ReplOutput::Value(v.display()),
                Err(e) => return ReplOutput::Error(format!("eval error: {e}")),
            }
        }

        // --- try as declaration(s) ---
        // Wrap in a module so parse_module can parse it.
        let wrapped = trimmed.to_string();
        let fid2 = self.next_fid();
        self.sm.add(format!("<repl:{}>", fid2.0), wrapped.clone());
        match parse_module(fid2, &wrapped) {
            Ok(m) if !m.decls.is_empty() => {
                let names: Vec<String> = m.decls.iter().map(|d| d.name().to_string()).collect();
                for d in m.decls {
                    // Overwrite any existing decl with the same name.
                    self.module
                        .decls
                        .retain(|existing| existing.name() != d.name());
                    self.module.decls.push(d);
                }
                ReplOutput::Defined(format!("defined: {}", names.join(", ")))
            }
            Ok(_) => ReplOutput::Error("no declarations found".to_string()),
            Err(e) => ReplOutput::Error(format!("parse error: {e}")),
        }
    }

    fn cmd_type(&mut self, src: &str) -> ReplOutput {
        let fid = self.next_fid();
        self.sm
            .add(format!("<repl:type:{}>", fid.0), src.to_string());
        // Wrap the expression in a synthetic fn to type-check it.
        let wrapped = format!("fn __repl_t__() -> _ effects {{}} {{ {src} }}");
        let fid2 = self.next_fid();
        self.sm
            .add(format!("<repl:type2:{}>", fid2.0), wrapped.clone());
        // Merge with session module.
        let mut merged_src = session_preamble(&self.module);
        merged_src.push_str(&wrapped);
        let fid3 = self.next_fid();
        self.sm
            .add(format!("<repl:typecheck:{}>", fid3.0), merged_src.clone());
        match parse_module(fid3, &merged_src) {
            Ok(m) => {
                let (_ctx, diags) = check_module(&m);
                let errors: Vec<_> = diags
                    .iter()
                    .filter(|d| d.severity == Severity::Error)
                    .collect();
                if !errors.is_empty() {
                    return ReplOutput::Error(
                        errors
                            .iter()
                            .map(|d| d.msg.clone())
                            .collect::<Vec<_>>()
                            .join("; "),
                    );
                }
                // Find __repl_t__ in the parsed module and report its declared return type.
                if let Some(Decl::Fn(f)) = m.decls.iter().find(|d| d.name() == "__repl_t__") {
                    ReplOutput::Type(format!("{src} : {:?}", f.ret))
                } else {
                    ReplOutput::Type(format!("{src} : <unknown>"))
                }
            }
            Err(e) => ReplOutput::Error(format!("parse error: {e}")),
        }
    }

    fn cmd_debug(&mut self, src: &str) -> ReplOutput {
        let fid = self.next_fid();
        self.sm
            .add(format!("<repl:debug:{}>", fid.0), src.to_string());
        match parse_expr(fid, src) {
            Ok(expr) => ReplOutput::Debug(format!("{expr:#?}")),
            Err(e) => ReplOutput::Error(format!("parse error: {e}")),
        }
    }

    fn cmd_prov(&mut self, src: &str) -> ReplOutput {
        let fid = self.next_fid();
        self.sm
            .add(format!("<repl:prov:{}>", fid.0), src.to_string());
        match parse_expr(fid, src) {
            Ok(expr) => {
                let mut rt = Runtime::new(self.module.clone());
                rt.capture_only = true;
                match rt.eval_root(&expr) {
                    Ok(v) => {
                        let chain = v.prov();
                        let nodes = chain.nodes_topo();
                        let mut out = String::from("provenance:\n");
                        for (id, n) in nodes {
                            out.push_str(&format!("  #{id}: {:?}", n.op));
                            if !n.parents.is_empty() {
                                out.push_str(&format!("  <- {:?}", n.parents));
                            }
                            out.push('\n');
                        }
                        ReplOutput::Prov(out)
                    }
                    Err(e) => ReplOutput::Error(format!("eval error: {e}")),
                }
            }
            Err(e) => ReplOutput::Error(format!("parse error: {e}")),
        }
    }

    fn cmd_load(&mut self, path: &str) -> ReplOutput {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => return ReplOutput::Error(format!("could not read `{path}`: {e}")),
        };
        let fid = self.next_fid();
        self.sm.add(path.to_string(), src.clone());
        match parse_module(fid, &src) {
            Ok(m) => {
                let count = m.decls.len();
                for d in m.decls {
                    self.module.decls.retain(|e| e.name() != d.name());
                    self.module.decls.push(d);
                }
                ReplOutput::Loaded(format!("loaded {count} declaration(s) from `{path}`"))
            }
            Err(e) => ReplOutput::Error(format!("parse error in `{path}`: {e}")),
        }
    }
}

/// Reconstruct a minimal source preamble from the session module's declarations.
/// Used for type-checking expressions in context.
fn session_preamble(module: &Module) -> String {
    use aether_parser::pretty::{module as pp_module, Form};
    // Re-pretty-print all current declarations as source.
    // We build a synthetic module to pretty-print.
    let tmp = Module {
        name: None,
        doc: None,
        decls: module.decls.clone(),
        span: module.span,
    };
    let mut s = pp_module(&tmp, Form::Verbose);
    s.push('\n');
    s
}

/// Run the interactive REPL using rustyline for line editing.
pub fn run_interactive(version: &str) -> anyhow::Result<()> {
    use rustyline::error::ReadlineError;
    use rustyline::DefaultEditor;

    println!("Aether REPL v{version}");
    println!("Type :h for help, :q to quit.");
    println!();

    let mut rl = DefaultEditor::new()?;
    let mut engine = Engine::new();

    loop {
        match rl.readline("aether> ") {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;
                match engine.feed(&line) {
                    ReplOutput::Quit => {
                        println!("Bye.");
                        break;
                    }
                    ReplOutput::Empty => {}
                    ReplOutput::Value(v) => println!("{v}"),
                    ReplOutput::Defined(msg) => println!("{msg}"),
                    ReplOutput::Type(t) => println!("{t}"),
                    ReplOutput::Debug(d) => println!("{d}"),
                    ReplOutput::Prov(p) => print!("{p}"),
                    ReplOutput::Loaded(msg) => println!("{msg}"),
                    ReplOutput::Help(h) => println!("{h}"),
                    ReplOutput::Error(e) => eprintln!("error: {e}"),
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("Bye.");
                break;
            }
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repl_arithmetic() {
        let mut e = Engine::new();
        match e.feed("1 + 2") {
            ReplOutput::Value(v) => assert_eq!(v, "3"),
            other => panic!("expected Value(3), got {other:?}"),
        }
    }

    #[test]
    fn repl_define_and_call() {
        let mut e = Engine::new();
        match e.feed("fn double(x: Int) -> Int effects {} { x + x }") {
            ReplOutput::Defined(msg) => assert!(msg.contains("double"), "got: {msg}"),
            other => panic!("expected Defined, got {other:?}"),
        }
        match e.feed("double(21)") {
            ReplOutput::Value(v) => assert_eq!(v, "42"),
            other => panic!("expected Value(42), got {other:?}"),
        }
    }

    #[test]
    fn repl_quit() {
        let mut e = Engine::new();
        assert!(matches!(e.feed(":q"), ReplOutput::Quit));
    }

    #[test]
    fn repl_help() {
        let mut e = Engine::new();
        match e.feed(":h") {
            ReplOutput::Help(h) => assert!(h.contains(":q")),
            other => panic!("expected Help, got {other:?}"),
        }
    }

    #[test]
    fn repl_empty_line() {
        let mut e = Engine::new();
        assert!(matches!(e.feed(""), ReplOutput::Empty));
        assert!(matches!(e.feed("   "), ReplOutput::Empty));
    }

    #[test]
    fn repl_debug() {
        let mut e = Engine::new();
        match e.feed(":d 1 + 2") {
            ReplOutput::Debug(d) => assert!(d.contains("Bin")),
            other => panic!("expected Debug, got {other:?}"),
        }
    }

    #[test]
    fn repl_prov() {
        let mut e = Engine::new();
        match e.feed(":p 1 + 2") {
            ReplOutput::Prov(p) => assert!(p.contains("BinOp")),
            other => panic!("expected Prov, got {other:?}"),
        }
    }

    #[test]
    fn repl_persist_across_inputs() {
        let mut e = Engine::new();
        e.feed("fn inc(n: Int) -> Int effects {} { n + 1 }");
        e.feed("fn dec(n: Int) -> Int effects {} { n + -1 }");
        match e.feed("inc(dec(10))") {
            ReplOutput::Value(v) => assert_eq!(v, "10"),
            other => panic!("expected Value(10), got {other:?}"),
        }
    }
}
