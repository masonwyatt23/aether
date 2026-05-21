//! Aether standard library.
//!
//! The agent primitives (`introspect`, `summarize`, `provenance`,
//! `confident`, `assume`) are implemented as builtins inside `aether-eval`.
//! This crate hosts higher-level agent workflows written in Aether itself
//! as `.ae` source files under `crates/aether-stdlib/aether/`.
//!
//! The `STD_SOURCE` constant (and `STD_MODULES` / `prelude_modules()`) are
//! what the CLI prepends to user programs when `--no-prelude` is not set.

// ── individual module sources ──────────────────────────────────────────────

const PLAN_SRC: &str = include_str!("../aether/std/plan.ae");
const ITER_SRC: &str = include_str!("../aether/std/iter.ae");
const MEM_SRC: &str = include_str!("../aether/std/mem.ae");
const PROOF_SRC: &str = include_str!("../aether/std/proof.ae");
const JSON_SRC: &str = include_str!("../aether/std/json.ae");
const LIST_SRC: &str = include_str!("../aether/std/list.ae");
const STRING_SRC: &str = include_str!("../aether/std/string.ae");
const MAP_SRC: &str = include_str!("../aether/std/map.ae");
const PATH_SRC: &str = include_str!("../aether/std/path.ae");
const TIME_SRC: &str = include_str!("../aether/std/time.ae");
const ENV_SRC: &str = include_str!("../aether/std/env.ae");
const FMT_SRC: &str = include_str!("../aether/std/fmt.ae");
const RESULT_SRC: &str = include_str!("../aether/std/result.ae");
const REGEX_SRC: &str = include_str!("../aether/std/regex.ae");
const SYS_SRC: &str = include_str!("../aether/std/sys.ae");
const MATH_SRC: &str = include_str!("../aether/std/math.ae");
const BASE64_SRC: &str = include_str!("../aether/std/base64.ae");
const HASH_SRC: &str = include_str!("../aether/std/hash.ae");
const UUID_SRC: &str = include_str!("../aether/std/uuid.ae");
const RANDOM_SRC: &str = include_str!("../aether/std/random.ae");
const DATE_SRC: &str = include_str!("../aether/std/date.ae");
const LOG_SRC: &str = include_str!("../aether/std/log.ae");
const TERM_SRC: &str = include_str!("../aether/std/term.ae");
const YAML_SRC: &str = include_str!("../aether/std/yaml.ae");
const FS_SRC: &str = include_str!("../aether/std/fs.ae");
const CACHE_SRC: &str = include_str!("../aether/std/cache.ae");
const RETRY_SRC: &str = include_str!("../aether/std/retry.ae");
const HTTP_SERVER_SRC: &str = include_str!("../aether/std/http_server.ae");
const STRLIST_SRC: &str = include_str!("../aether/std/strlist.ae");

// ── public surface ─────────────────────────────────────────────────────────

/// All stdlib modules as `(qualified_name, source)` pairs.
///
/// The qualified name is the module path an agent would use in an `import`
/// declaration, e.g. `import std::plan`.
pub const STD_MODULES: &[(&str, &str)] = &[
    ("std::plan", PLAN_SRC),
    ("std::iter", ITER_SRC),
    ("std::mem", MEM_SRC),
    ("std::proof", PROOF_SRC),
    ("std::json", JSON_SRC),
    ("std::list", LIST_SRC),
    ("std::string", STRING_SRC),
    ("std::map", MAP_SRC),
    ("std::path", PATH_SRC),
    ("std::time", TIME_SRC),
    ("std::env", ENV_SRC),
    ("std::fmt", FMT_SRC),
    ("std::result", RESULT_SRC),
    ("std::regex", REGEX_SRC),
    ("std::sys", SYS_SRC),
    ("std::math", MATH_SRC),
    ("std::base64", BASE64_SRC),
    ("std::hash", HASH_SRC),
    ("std::uuid", UUID_SRC),
    ("std::random", RANDOM_SRC),
    ("std::date", DATE_SRC),
    ("std::log", LOG_SRC),
    ("std::term", TERM_SRC),
    ("std::yaml", YAML_SRC),
    ("std::fs", FS_SRC),
    ("std::cache", CACHE_SRC),
    ("std::retry", RETRY_SRC),
    ("std::http_server", HTTP_SERVER_SRC),
    ("std::strlist", STRLIST_SRC),
];

/// Return the full `STD_MODULES` slice (ergonomic alias for dynamic callers).
pub fn prelude_modules() -> &'static [(&'static str, &'static str)] {
    STD_MODULES
}

/// Concatenation of the core prelude, used as the implicit preamble when
/// the CLI runs without `--no-prelude`. Individual modules in `STD_MODULES`
/// are preferred for large programs (parse only what you import).
pub const STD_SOURCE: &str = r#"
## Aether standard library prelude.
## Higher-level helpers live in std.* modules; the agent primitives
## (introspect, summarize, provenance, confident, assume) are language built-ins.

## Run `step` `budget` times, threading the seed through each invocation.
fn iter(seed: Int, step: Str, budget: Int) -> Int effects {} {
  iter_refine(seed, step, budget)
}
"#;

/// Names exported by the prelude (declared and reserved).
pub fn prelude_names() -> &'static [&'static str] {
    &[
        // Agent primitives (also keywords in the grammar)
        "introspect",
        "summarize",
        "provenance",
        "confident",
        "assume",
        "spec",
        // Stdlib builtins implemented in aether-eval::builtins
        "print",
        "println",
        "str",
        "int",
        "len",
        "abs",
        "max",
        "min",
        "iter_refine",
        "print_module_surface",
        "print_prov",
        "http_get",
    ]
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::FileId;
    use aether_parser::parse_module;
    use aether_types::{check_module, Severity};

    /// Every stdlib module must parse without error and type-check without
    /// any Error-severity diagnostic.
    #[test]
    fn stdlib_modules_parse_and_typecheck() {
        for (name, src) in STD_MODULES {
            let module = parse_module(FileId(0), src)
                .unwrap_or_else(|e| panic!("parse error in {name}: {e}"));

            let (_, diags) = check_module(&module);

            let errors: Vec<_> = diags
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .collect();

            assert!(errors.is_empty(), "type errors in {name}: {errors:#?}");
        }
    }

    /// `prelude_modules()` and `STD_MODULES` must return identical contents.
    #[test]
    fn prelude_modules_matches_const() {
        assert_eq!(prelude_modules().len(), STD_MODULES.len());
        for ((n1, s1), (n2, s2)) in prelude_modules().iter().zip(STD_MODULES.iter()) {
            assert_eq!(n1, n2);
            assert_eq!(s1, s2);
        }
    }

    /// STD_SOURCE must still compile on its own (backward-compat guard).
    #[test]
    fn std_source_parses() {
        parse_module(FileId(0), STD_SOURCE).expect("STD_SOURCE failed to parse");
    }

    /// Verify module names follow the std:: convention.
    #[test]
    fn module_names_have_std_prefix() {
        for (name, _) in STD_MODULES {
            assert!(
                name.starts_with("std::"),
                "module name {name:?} does not start with 'std::'"
            );
        }
    }

    /// Each module source must be non-empty.
    #[test]
    fn module_sources_non_empty() {
        for (name, src) in STD_MODULES {
            assert!(!src.is_empty(), "module {name} has empty source");
        }
    }
}
