//! Multi-file module loader and import resolver.
//!
//! Resolves `import` declarations by consulting:
//!   1. `aether_stdlib::STD_MODULES` (built-in stdlib, e.g. `std::iter`).
//!   2. `<project_root>/<path/with/slashes>.ae` (user modules).
//!
//! Transitive imports are handled via DFS with cycle detection.
//! The result of `Loader::load` is a *merged* `Module` whose `decls` contain
//! all imported `Fn`, `Let`, `TypeAlias`, and `Tool` declarations followed by
//! the entry file's own declarations.  All `Import` decls are stripped from the
//! merged result so the type-checker and evaluator see a flat module.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use aether_ast::{FileId, Module, SourceMap};
use aether_ast::decl::Decl;
use aether_parser::parse_module;

// ── error type ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum LoadError {
    /// Could not read a source file from disk.
    Io(String, std::io::Error),
    /// Parse failure; message and optional span encoded as "file:start..end".
    Parse(String),
    /// Import cycle: the path vector traces the cycle from entry to re-entry.
    Cycle(Vec<String>),
    /// No stdlib module and no file found for the given qualified name.
    NotFound(String),
    /// Alias syntax used; not yet supported (non-fatal warning promoted to error).
    AliasNotSupported(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(path, e) => write!(f, "could not read `{path}`: {e}"),
            LoadError::Parse(msg) => write!(f, "parse error: {msg}"),
            LoadError::Cycle(chain) => {
                write!(f, "import cycle detected: {}", chain.join(" -> "))
            }
            LoadError::NotFound(name) => {
                write!(f, "unknown module `{name}` (not in stdlib and no file found)")
            }
            LoadError::AliasNotSupported(name) => {
                write!(
                    f,
                    "import alias (`import {name} as …`) is not yet supported; \
                     omit the `as` clause"
                )
            }
        }
    }
}

impl std::error::Error for LoadError {}

// ── Loader ───────────────────────────────────────────────────────────────────

/// Loads an entry `.ae` file together with all of its (transitive) imports,
/// returning a single flat merged `Module`.
pub struct Loader {
    project_root: PathBuf,
    /// Cache: canonical_key -> already-loaded decls so we don't re-parse.
    cache: HashMap<String, Vec<Decl>>,
}

impl Loader {
    /// Create a loader rooted at `project_root`.
    /// For a single-file program pass the directory that contains that file.
    pub fn new(project_root: &Path) -> Self {
        Loader {
            project_root: project_root.to_path_buf(),
            cache: HashMap::new(),
        }
    }

    /// Parse `entry`, resolve all imports transitively, and return a merged
    /// flat `Module`.  Any `Import` decls in the merged module are stripped.
    pub fn load(
        &mut self,
        entry: &Path,
        sm: &mut SourceMap,
    ) -> Result<Module, LoadError> {
        let mut visiting: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();
        self.load_file(entry, sm, &mut visiting, &mut stack)
    }

    // ── internal ─────────────────────────────────────────────────────────────

    /// Load a single file (entry or import), returning the merged Module that
    /// includes all its transitive imports merged in.
    fn load_file(
        &mut self,
        path: &Path,
        sm: &mut SourceMap,
        visiting: &mut HashSet<String>,
        stack: &mut Vec<String>,
    ) -> Result<Module, LoadError> {
        let canonical = canonical_key_for_path(path);

        // Cycle detection.
        if visiting.contains(&canonical) {
            let mut chain = stack.clone();
            chain.push(canonical.clone());
            return Err(LoadError::Cycle(chain));
        }

        visiting.insert(canonical.clone());
        stack.push(canonical.clone());

        let src = std::fs::read_to_string(path)
            .map_err(|e| LoadError::Io(path.display().to_string(), e))?;

        let fid = sm.add(path.display().to_string(), src.clone());
        let module = parse_file(fid, &src)?;

        // Collect the root span and metadata before we move pieces around.
        let module_span = module.span;
        let module_name = module.name.clone();
        let module_doc = module.doc.clone();

        let mut merged: Vec<Decl> = Vec::new();

        for decl in module.decls {
            match decl {
                Decl::Import(ref imp) => {
                    // Alias check (non-fatal → clean error).
                    if imp.alias.is_some() {
                        let qname = imp.path.join("::");
                        visiting.remove(&canonical);
                        stack.pop();
                        return Err(LoadError::AliasNotSupported(qname));
                    }

                    let qname = imp.path.join("::");

                    // Selective import note: we import everything but mention it.
                    // (The spec says "emit a note diagnostic" — we print to stderr
                    //  so the user sees it, but do not fail.)
                    if !imp.names.is_empty() {
                        eprintln!(
                            "note: selective import `{{{}}} from {qname}` is not yet \
                             enforced — all declarations from `{qname}` will be imported",
                            imp.names.join(", ")
                        );
                    }

                    // Check cache first.
                    if let Some(cached) = self.cache.get(&qname) {
                        merged.extend(cached.iter().cloned());
                        continue;
                    }

                    // Resolve: stdlib first, then filesystem.
                    let import_decls =
                        if let Some(std_src) = stdlib_source(&qname) {
                            load_stdlib_module(&qname, std_src, sm)?
                        } else {
                            // Map "foo::bar::baz" -> "<root>/foo/bar/baz.ae"
                            let rel: PathBuf = imp.path.iter().collect::<PathBuf>()
                                .with_extension("ae");
                            let abs = self.project_root.join(&rel);
                            if !abs.exists() {
                                visiting.remove(&canonical);
                                stack.pop();
                                return Err(LoadError::NotFound(qname));
                            }
                            let sub = self.load_file(&abs, sm, visiting, stack)?;
                            sub.decls
                        };

                    self.cache.insert(qname.clone(), import_decls.clone());
                    merged.extend(import_decls);
                }
                // Non-import decls come after imports so imported names are
                // available when the entry's own code is type-checked.
                other => merged.push(other),
            }
        }

        visiting.remove(&canonical);
        stack.pop();

        Ok(Module {
            name: module_name,
            doc: module_doc,
            decls: merged,
            span: module_span,
        })
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Parse a file and wrap any parse error with our `LoadError::Parse`.
fn parse_file(fid: FileId, src: &str) -> Result<Module, LoadError> {
    parse_module(fid, src).map_err(|e| LoadError::Parse(format!("{e}")))
}

/// Load a stdlib module from its embedded source string.
/// Stdlib modules never import anything themselves, so no DFS needed.
fn load_stdlib_module(
    name: &str,
    src: &'static str,
    sm: &mut SourceMap,
) -> Result<Vec<Decl>, LoadError> {
    let fid = sm.add(format!("<stdlib:{name}>"), src);
    let module = parse_file(fid, src)?;
    // Strip any Import decls (stdlib modules currently have none, but be safe).
    let decls = module
        .decls
        .into_iter()
        .filter(|d| !matches!(d, Decl::Import(_)))
        .collect();
    Ok(decls)
}

/// Lookup `name` in `aether_stdlib::STD_MODULES`.
fn stdlib_source(name: &str) -> Option<&'static str> {
    aether_stdlib::STD_MODULES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, src)| *src)
}

/// Canonical cache key for a filesystem path: use the canonical (resolved)
/// absolute path string, or fall back to the display string.
fn canonical_key_for_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::SourceMap;

    #[test]
    fn stdlib_source_finds_iter() {
        assert!(stdlib_source("std::iter").is_some());
    }

    #[test]
    fn stdlib_source_misses_unknown() {
        assert!(stdlib_source("std::nonexistent").is_none());
    }

    #[test]
    fn load_error_display_cycle() {
        let e = LoadError::Cycle(vec!["a.ae".into(), "b.ae".into(), "a.ae".into()]);
        assert!(e.to_string().contains("cycle"));
    }

    #[test]
    fn load_stdlib_iter_directly() {
        let mut sm = SourceMap::new();
        let src = stdlib_source("std::iter").unwrap();
        let decls = load_stdlib_module("std::iter", src, &mut sm).unwrap();
        // std::iter exports at least clamp, count_to, refine_with, in_range, etc.
        assert!(!decls.is_empty());
        let has_clamp = decls.iter().any(|d| d.name() == "clamp");
        assert!(has_clamp, "expected clamp in std::iter");
    }
}
