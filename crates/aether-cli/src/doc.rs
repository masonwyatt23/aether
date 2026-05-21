//! `aether doc <file.ae>` — emit a Markdown document derived from the module surface.
//!
//! Sections:
//!   # <module name>                  (from module doc)
//!   > <doc string>                   (module-level docstring if present)
//!
//!   ### <name>                       one per fn / tool / let / type
//!   **Signature**: `signature`
//!   **Effects**: `{Eff1, Eff2}`      (omitted when empty)
//!   doc body text                    (if present)
//!
//! Output goes to stdout by default, or to the file given by `-o`.

use std::path::PathBuf;

use aether_ast::Expr;
use aether_ast::SourceMap;
use aether_eval::{ModuleSurface, Runtime};
use aether_parser::parse_module;
use aether_types::{check_module, Severity};

pub fn run_doc(file: PathBuf, output: Option<PathBuf>) -> anyhow::Result<()> {
    let src = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("could not read `{}`: {e}", file.display()))?;

    let mut sm = SourceMap::new();
    let fid = sm.add(file.display().to_string(), src.clone());

    let m = parse_module(fid, &src).map_err(|e| anyhow::anyhow!("parse error: {e}"))?;

    // Type-check; bail on errors but only warn about warnings.
    let (_, diags) = check_module(&m);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("error: {}", e.msg);
        }
        return Err(anyhow::anyhow!("{} type error(s)", errors.len()));
    }

    // Use the Runtime + introspect builtin to build the surface.
    let surface = build_surface(m)?;
    let md = render_markdown(&surface);

    match output {
        Some(path) => {
            std::fs::write(&path, &md)
                .map_err(|e| anyhow::anyhow!("could not write `{}`: {e}", path.display()))?;
            eprintln!("wrote `{}`", path.display());
        }
        None => print!("{md}"),
    }
    Ok(())
}

fn build_surface(m: aether_ast::Module) -> anyhow::Result<ModuleSurface> {
    let mut rt = Runtime::new(m);
    rt.capture_only = true;
    let call = Expr::Call {
        callee: Box::new(Expr::Var("introspect".into(), aether_ast::Span::DUMMY)),
        args: vec![aether_ast::Arg {
            name: None,
            value: Expr::Lit(
                aether_ast::Lit::Str("current".into()),
                aether_ast::Span::DUMMY,
            ),
            span: aether_ast::Span::DUMMY,
        }],
        span: aether_ast::Span::DUMMY,
    };
    match rt.eval_root(&call) {
        Ok(aether_eval::Value::ModuleSurface(s, _)) => Ok(s),
        Ok(v) => Err(anyhow::anyhow!(
            "unexpected introspect result: {}",
            v.display()
        )),
        Err(e) => Err(anyhow::anyhow!("eval error: {e}")),
    }
}

fn render_markdown(s: &ModuleSurface) -> String {
    let mut out = String::new();

    // Title
    out.push_str("# ");
    out.push_str(&s.name);
    out.push('\n');

    // Module docstring
    if let Some(doc) = &s.doc {
        out.push('\n');
        out.push_str(doc.trim());
        out.push('\n');
    }

    if !s.exports.is_empty() {
        out.push('\n');
    }

    for entry in &s.exports {
        out.push_str("### ");
        out.push_str(&entry.name);
        out.push('\n');
        out.push('\n');
        out.push_str("**Signature**: `");
        out.push_str(&entry.name);
        out.push_str(&entry.signature);
        out.push_str("`\n");
        if !entry.effects.is_empty() {
            out.push_str("\n**Effects**: `{");
            out.push_str(&entry.effects.join(", "));
            out.push_str("}`\n");
        }
        if let Some(doc) = &entry.doc {
            out.push('\n');
            out.push_str(doc.trim());
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_markdown_basic() {
        use aether_eval::{ExportEntry, ModuleSurface};
        let surface = ModuleSurface {
            name: "greet".to_string(),
            doc: Some("A greeting module.".to_string()),
            exports: vec![ExportEntry {
                name: "hello".to_string(),
                kind: "fn",
                signature: "(name: Str) -> Str".to_string(),
                effects: vec![],
                doc: Some("Returns a greeting.".to_string()),
            }],
        };
        let md = render_markdown(&surface);
        assert!(md.contains("# greet"), "title missing");
        assert!(md.contains("A greeting module."), "module doc missing");
        assert!(md.contains("### hello"), "fn section missing");
        assert!(md.contains("Returns a greeting."), "fn doc missing");
    }
}
