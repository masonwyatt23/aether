//! Built-in function signatures registered before user code is checked.

use crate::ctx::{FnSig, TypeCtx};
use aether_ast::*;

fn ty_con(c: TyCon) -> Type {
    Type::Con(c, Span::DUMMY)
}

fn sig(params: Vec<(&str, Type)>, ret: Type, effects: Vec<Effect>) -> FnSig {
    FnSig {
        params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),
        ret,
        effects: EffectRow::from_iter(effects),
        requires: vec![],
        ensures: vec![],
        span: Span::DUMMY,
    }
}

pub fn install_builtins(ctx: &mut TypeCtx) {
    use Effect::*;
    use TyCon::*;

    // print(value) - IO
    ctx.insert_fn(
        "print".to_string(),
        sig(vec![("value", ty_con(Str))], ty_con(Unit), vec![IO]),
    );

    // assert(cond) - Throw if false. Used by test {} blocks.
    ctx.insert_fn(
        "assert".to_string(),
        sig(vec![("cond", ty_con(Bool))], ty_con(Unit), vec![Throw]),
    );

    // assert_eq(a, b) - Throw if a != b. Used by test {} blocks.
    ctx.insert_fn(
        "assert_eq".to_string(),
        sig(
            vec![
                ("a", Type::Var("t".into(), Span::DUMMY)),
                ("b", Type::Var("t".into(), Span::DUMMY)),
            ],
            ty_con(Unit),
            vec![Throw],
        ),
    );

    // snap_expect(label, value) - Collect snapshot entry. Used by snap {} blocks.
    ctx.insert_fn(
        "snap_expect".to_string(),
        sig(
            vec![("label", ty_con(Str)), ("value", ty_con(Str))],
            ty_con(Unit),
            vec![Throw, FS],
        ),
    );

    // llm_complete(prompt) -> Str ~ confidence(p) — agent-native completion tool.
    // Backed by a default deterministic stub; real LLM dispatch can be registered
    // via Runtime::tools.register("llm_complete", ...).
    ctx.insert_fn(
        "llm_complete".to_string(),
        sig(vec![("prompt", ty_con(Str))], ty_con(Str), vec![Net, Throw]),
    );
    // println(value) - IO. Convenience.
    ctx.insert_fn(
        "println".to_string(),
        sig(vec![("value", ty_con(Str))], ty_con(Unit), vec![IO]),
    );

    // str(value) -> Str — coerces any base value to a string. We weaken its type
    // to accept Int / Float / Bool / Str via the `_any` sentinel using a tvar.
    ctx.insert_fn(
        "str".to_string(),
        sig(
            vec![("value", Type::Var("a".into(), Span::DUMMY))],
            ty_con(Str),
            vec![],
        ),
    );

    // int(value) -> Int  (coercion / parse)
    ctx.insert_fn(
        "int".to_string(),
        sig(vec![("value", ty_con(Str))], ty_con(Int), vec![Throw]),
    );

    // len(list-or-str) -> Int
    ctx.insert_fn(
        "len".to_string(),
        sig(vec![("value", Type::Var("a".into(), Span::DUMMY))], ty_con(Int), vec![]),
    );

    // introspect(target [, depth]) -> ModuleSurface
    ctx.insert_fn(
        "introspect".to_string(),
        sig(
            vec![
                ("target", ty_con(Str)),
                ("depth", ty_con(Int)),
            ],
            ty_con(ModuleSurface),
            vec![],
        ),
    );

    // summarize(scope, budget) -> Str
    ctx.insert_fn(
        "summarize".to_string(),
        sig(
            vec![("scope", ty_con(Str)), ("budget", ty_con(Int))],
            ty_con(Str),
            vec![],
        ),
    );

    // provenance(value) -> ProvChain
    ctx.insert_fn(
        "provenance".to_string(),
        sig(
            vec![("value", Type::Var("a".into(), Span::DUMMY))],
            ty_con(ProvChain),
            vec![],
        ),
    );

    // print_module_surface(s) - IO
    ctx.insert_fn(
        "print_module_surface".to_string(),
        sig(vec![("surface", ty_con(ModuleSurface))], ty_con(Unit), vec![IO]),
    );

    // print_prov(chain) - IO
    ctx.insert_fn(
        "print_prov".to_string(),
        sig(vec![("chain", ty_con(ProvChain))], ty_con(Unit), vec![IO]),
    );

    // http_get(url) -> Str — example tool. Net + Throw.
    ctx.insert_fn(
        "http_get".to_string(),
        sig(vec![("url", ty_con(Str))], ty_con(Str), vec![Net, Throw]),
    );

    // Math helpers — pure.
    ctx.insert_fn(
        "abs".to_string(),
        sig(vec![("n", ty_con(Int))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "max".to_string(),
        sig(vec![("a", ty_con(Int)), ("b", ty_con(Int))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "min".to_string(),
        sig(vec![("a", ty_con(Int)), ("b", ty_con(Int))], ty_con(Int), vec![]),
    );

    // std.iter.refine(seed, step, budget) - iterate `step` budget times.
    ctx.insert_fn(
        "iter_refine".to_string(),
        sig(
            vec![
                ("seed", Type::Var("a".into(), Span::DUMMY)),
                ("step", Type::Var("a".into(), Span::DUMMY)),
                ("budget", ty_con(Int)),
            ],
            Type::Var("a".into(), Span::DUMMY),
            vec![],
        ),
    );

    // ── std::list natives ────────────────────────────────────────────────────

    ctx.insert_fn(
        "list_sum_native".to_string(),
        sig(vec![("xs", Type::List(Box::new(ty_con(Int)), Span::DUMMY))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "list_max_native".to_string(),
        sig(vec![("xs", Type::List(Box::new(ty_con(Int)), Span::DUMMY))], ty_con(Int), vec![Throw]),
    );
    ctx.insert_fn(
        "list_contains_native".to_string(),
        sig(
            vec![
                ("xs", Type::List(Box::new(ty_con(Int)), Span::DUMMY)),
                ("x", ty_con(Int)),
            ],
            ty_con(Bool),
            vec![],
        ),
    );
    ctx.insert_fn(
        "list_count_native".to_string(),
        sig(
            vec![
                ("xs", Type::List(Box::new(ty_con(Int)), Span::DUMMY)),
                ("x", ty_con(Int)),
            ],
            ty_con(Int),
            vec![],
        ),
    );
    ctx.insert_fn(
        "list_reversed_native".to_string(),
        sig(
            vec![("xs", Type::List(Box::new(ty_con(Int)), Span::DUMMY))],
            Type::List(Box::new(ty_con(Int)), Span::DUMMY),
            vec![],
        ),
    );

    // ── std::strlist natives ─────────────────────────────────────────────────

    ctx.insert_fn(
        "strlist_len_native".to_string(),
        sig(vec![("xs", Type::List(Box::new(ty_con(Str)), Span::DUMMY))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "strlist_get_native".to_string(),
        sig(
            vec![
                ("xs", Type::List(Box::new(ty_con(Str)), Span::DUMMY)),
                ("i", ty_con(Int)),
            ],
            ty_con(Str),
            vec![Throw],
        ),
    );
    ctx.insert_fn(
        "strlist_join_native".to_string(),
        sig(
            vec![
                ("xs", Type::List(Box::new(ty_con(Str)), Span::DUMMY)),
                ("sep", ty_con(Str)),
            ],
            ty_con(Str),
            vec![],
        ),
    );
    ctx.insert_fn(
        "strlist_contains_native".to_string(),
        sig(
            vec![
                ("xs", Type::List(Box::new(ty_con(Str)), Span::DUMMY)),
                ("x", ty_con(Str)),
            ],
            ty_con(Bool),
            vec![],
        ),
    );
    ctx.insert_fn(
        "strlist_reversed_native".to_string(),
        sig(
            vec![("xs", Type::List(Box::new(ty_con(Str)), Span::DUMMY))],
            Type::List(Box::new(ty_con(Str)), Span::DUMMY),
            vec![],
        ),
    );
    ctx.insert_fn(
        "strlist_head_native".to_string(),
        sig(vec![("xs", Type::List(Box::new(ty_con(Str)), Span::DUMMY))], ty_con(Str), vec![Throw]),
    );
    ctx.insert_fn(
        "strlist_tail_native".to_string(),
        sig(
            vec![("xs", Type::List(Box::new(ty_con(Str)), Span::DUMMY))],
            Type::List(Box::new(ty_con(Str)), Span::DUMMY),
            vec![],
        ),
    );

    // ── std::string natives ──────────────────────────────────────────────────

    ctx.insert_fn(
        "str_upper_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "str_lower_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "str_contains_native".to_string(),
        sig(vec![("s", ty_con(Str)), ("needle", ty_con(Str))], ty_con(Bool), vec![]),
    );
    ctx.insert_fn(
        "str_starts_with_native".to_string(),
        sig(vec![("s", ty_con(Str)), ("prefix", ty_con(Str))], ty_con(Bool), vec![]),
    );
    ctx.insert_fn(
        "str_split_on_native".to_string(),
        sig(
            vec![("s", ty_con(Str)), ("sep", ty_con(Str))],
            Type::List(Box::new(ty_con(Str)), Span::DUMMY),
            vec![],
        ),
    );
    ctx.insert_fn(
        "str_join_native".to_string(),
        sig(
            vec![
                ("xs", Type::List(Box::new(ty_con(Str)), Span::DUMMY)),
                ("sep", ty_con(Str)),
            ],
            ty_con(Str),
            vec![],
        ),
    );
    ctx.insert_fn(
        "str_trim_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );

    // ── std::map natives ─────────────────────────────────────────────────────
    // MapEntry = {key: Str, value: Int}. The type-checker doesn't expand aliases
    // in call sites, so we use a type variable for the list parameter (same
    // approach as `len`). The .ae wrapper enforces [MapEntry] at the Aether level.

    let map_list_var = || Type::Var("m".into(), Span::DUMMY);

    ctx.insert_fn(
        "map_get_native".to_string(),
        sig(vec![("m", map_list_var()), ("key", ty_con(Str))], ty_con(Int), vec![Throw]),
    );
    ctx.insert_fn(
        "map_set_native".to_string(),
        sig(
            vec![("m", map_list_var()), ("key", ty_con(Str)), ("value", ty_con(Int))],
            map_list_var(),
            vec![],
        ),
    );
    ctx.insert_fn(
        "map_has_native".to_string(),
        sig(vec![("m", map_list_var()), ("key", ty_con(Str))], ty_con(Bool), vec![]),
    );

    // ── std::path natives ─────────────────────────────────────────────────────

    ctx.insert_fn(
        "path_join_native".to_string(),
        sig(vec![("a", ty_con(Str)), ("b", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "path_basename_native".to_string(),
        sig(vec![("p", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "path_dirname_native".to_string(),
        sig(vec![("p", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "path_extension_native".to_string(),
        sig(vec![("p", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "path_exists_native".to_string(),
        sig(vec![("p", ty_con(Str))], ty_con(Bool), vec![FS]),
    );

    // ── std::time natives ─────────────────────────────────────────────────────

    ctx.insert_fn(
        "time_now_ms_native".to_string(),
        sig(vec![], ty_con(Int), vec![IO]),
    );
    ctx.insert_fn(
        "time_monotonic_ms_native".to_string(),
        sig(vec![], ty_con(Int), vec![IO]),
    );
    ctx.insert_fn(
        "time_format_iso_native".to_string(),
        sig(vec![("ms", ty_con(Int))], ty_con(Str), vec![]),
    );

    // ── std::env natives ──────────────────────────────────────────────────────

    ctx.insert_fn(
        "env_get_native".to_string(),
        sig(vec![("name", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "env_has_native".to_string(),
        sig(vec![("name", ty_con(Str))], ty_con(Bool), vec![]),
    );
    ctx.insert_fn(
        "env_set_native".to_string(),
        sig(vec![("name", ty_con(Str)), ("value", ty_con(Str))], ty_con(Unit), vec![State]),
    );

    // ── std::fmt natives ──────────────────────────────────────────────────────

    ctx.insert_fn(
        "fmt1_native".to_string(),
        sig(vec![("template", ty_con(Str)), ("a", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "fmt2_native".to_string(),
        sig(
            vec![("template", ty_con(Str)), ("a", ty_con(Str)), ("b", ty_con(Str))],
            ty_con(Str),
            vec![],
        ),
    );
    ctx.insert_fn(
        "fmt3_native".to_string(),
        sig(
            vec![
                ("template", ty_con(Str)),
                ("a", ty_con(Str)),
                ("b", ty_con(Str)),
                ("c", ty_con(Str)),
            ],
            ty_con(Str),
            vec![],
        ),
    );
    ctx.insert_fn(
        "fmt4_native".to_string(),
        sig(
            vec![
                ("template", ty_con(Str)),
                ("a", ty_con(Str)),
                ("b", ty_con(Str)),
                ("c", ty_con(Str)),
                ("d", ty_con(Str)),
            ],
            ty_con(Str),
            vec![],
        ),
    );
    ctx.insert_fn(
        "fmt5_native".to_string(),
        sig(
            vec![
                ("template", ty_con(Str)),
                ("a", ty_con(Str)),
                ("b", ty_con(Str)),
                ("c", ty_con(Str)),
                ("d", ty_con(Str)),
                ("e", ty_con(Str)),
            ],
            ty_con(Str),
            vec![],
        ),
    );
    ctx.insert_fn(
        "fmt_list_native".to_string(),
        sig(
            vec![
                ("template", ty_con(Str)),
                ("args", Type::List(Box::new(ty_con(Str)), Span::DUMMY)),
            ],
            ty_con(Str),
            vec![],
        ),
    );
    ctx.insert_fn(
        "pad_left_native".to_string(),
        sig(
            vec![("s", ty_con(Str)), ("n", ty_con(Int)), ("ch", ty_con(Str))],
            ty_con(Str),
            vec![],
        ),
    );
    ctx.insert_fn(
        "pad_right_native".to_string(),
        sig(
            vec![("s", ty_con(Str)), ("n", ty_con(Int)), ("ch", ty_con(Str))],
            ty_con(Str),
            vec![],
        ),
    );

    // ── std::result natives ───────────────────────────────────────────────────
    // `result_throw_native` diverges — it always throws. We give it a return
    // type of `Type::Var("a")` so it unifies with whatever the surrounding
    // match arm expects (here `Str`, the return type of `result_unwrap`).

    ctx.insert_fn(
        "result_throw_native".to_string(),
        sig(
            vec![("msg", ty_con(Str))],
            Type::Var("a".into(), Span::DUMMY),
            vec![Throw],
        ),
    );

    // ── std::regex natives ────────────────────────────────────────────────────

    ctx.insert_fn(
        "regex_match_native".to_string(),
        sig(vec![("pattern", ty_con(Str)), ("input", ty_con(Str))], ty_con(Bool), vec![Throw]),
    );
    ctx.insert_fn(
        "regex_find_native".to_string(),
        sig(vec![("pattern", ty_con(Str)), ("input", ty_con(Str))], ty_con(Str), vec![Throw]),
    );
    ctx.insert_fn(
        "regex_replace_native".to_string(),
        sig(
            vec![("pattern", ty_con(Str)), ("input", ty_con(Str)), ("replacement", ty_con(Str))],
            ty_con(Str),
            vec![Throw],
        ),
    );
    ctx.insert_fn(
        "regex_replace_all_native".to_string(),
        sig(
            vec![("pattern", ty_con(Str)), ("input", ty_con(Str)), ("replacement", ty_con(Str))],
            ty_con(Str),
            vec![Throw],
        ),
    );
    ctx.insert_fn(
        "regex_split_native".to_string(),
        sig(
            vec![("pattern", ty_con(Str)), ("input", ty_con(Str))],
            Type::List(Box::new(ty_con(Str)), Span::DUMMY),
            vec![Throw],
        ),
    );
    ctx.insert_fn(
        "regex_captures_native".to_string(),
        sig(
            vec![("pattern", ty_con(Str)), ("input", ty_con(Str))],
            Type::List(Box::new(ty_con(Str)), Span::DUMMY),
            vec![Throw],
        ),
    );

    // ── std::sys natives ──────────────────────────────────────────────────────

    ctx.insert_fn(
        "sys_exit_native".to_string(),
        sig(vec![("code", ty_con(Int))], ty_con(Unit), vec![IO]),
    );
    ctx.insert_fn(
        "sys_args_native".to_string(),
        sig(vec![], Type::List(Box::new(ty_con(Str)), Span::DUMMY), vec![]),
    );
    ctx.insert_fn(
        "sys_stdin_line_native".to_string(),
        sig(vec![], ty_con(Str), vec![IO]),
    );
    ctx.insert_fn(
        "sys_now_unix_native".to_string(),
        sig(vec![], ty_con(Int), vec![IO]),
    );
    ctx.insert_fn(
        "sys_spawn_native".to_string(),
        sig(
            vec![
                ("cmd", ty_con(Str)),
                ("args", Type::List(Box::new(ty_con(Str)), Span::DUMMY)),
            ],
            ty_con(Str),
            vec![IO, Throw],
        ),
    );
    ctx.insert_fn(
        "sys_hostname_native".to_string(),
        sig(vec![], ty_con(Str), vec![IO]),
    );
    ctx.insert_fn(
        "sys_spawn_thread_sleep_native".to_string(),
        sig(vec![("ms", ty_con(Int))], ty_con(Unit), vec![IO]),
    );

    // ── std::math natives ─────────────────────────────────────────────────────

    ctx.insert_fn(
        "math_pow_native".to_string(),
        sig(vec![("base", ty_con(Int)), ("exp", ty_con(Int))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "math_sqrt_native".to_string(),
        sig(vec![("x", ty_con(Float))], ty_con(Float), vec![]),
    );
    ctx.insert_fn(
        "math_floor_native".to_string(),
        sig(vec![("x", ty_con(Float))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "math_ceil_native".to_string(),
        sig(vec![("x", ty_con(Float))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "math_round_native".to_string(),
        sig(vec![("x", ty_con(Float))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "math_min_f_native".to_string(),
        sig(vec![("a", ty_con(Float)), ("b", ty_con(Float))], ty_con(Float), vec![]),
    );
    ctx.insert_fn(
        "math_max_f_native".to_string(),
        sig(vec![("a", ty_con(Float)), ("b", ty_con(Float))], ty_con(Float), vec![]),
    );
    ctx.insert_fn(
        "math_pi_native".to_string(),
        sig(vec![], ty_con(Float), vec![]),
    );

    // ── std::base64 natives ───────────────────────────────────────────────────

    ctx.insert_fn(
        "base64_encode_native".to_string(),
        sig(vec![("input", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "base64_decode_native".to_string(),
        sig(vec![("input", ty_con(Str))], ty_con(Str), vec![Throw]),
    );

    // ── std::hash natives ─────────────────────────────────────────────────────

    ctx.insert_fn(
        "hash_default_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "hash_sha256_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );

    // ── std::uuid natives ─────────────────────────────────────────────────────

    ctx.insert_fn(
        "uuid_v4_native".to_string(),
        sig(vec![], ty_con(Str), vec![Rand]),
    );
    ctx.insert_fn(
        "uuid_short_native".to_string(),
        sig(vec![], ty_con(Str), vec![Rand]),
    );

    // ── std::random natives ───────────────────────────────────────────────────

    ctx.insert_fn(
        "random_int_native".to_string(),
        sig(vec![("lo", ty_con(Int)), ("hi", ty_con(Int))], ty_con(Int), vec![Rand]),
    );
    ctx.insert_fn(
        "random_bool_native".to_string(),
        sig(vec![], ty_con(Bool), vec![Rand]),
    );
    ctx.insert_fn(
        "random_float_native".to_string(),
        sig(vec![], ty_con(Float), vec![Rand]),
    );
    ctx.insert_fn(
        "random_pick_native".to_string(),
        sig(
            vec![("xs", Type::List(Box::new(ty_con(Str)), Span::DUMMY))],
            ty_con(Str),
            vec![Rand, Throw],
        ),
    );

    // ── std::date natives ─────────────────────────────────────────────────────

    ctx.insert_fn(
        "date_year_native".to_string(),
        sig(vec![("ms", ty_con(Int))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "date_month_native".to_string(),
        sig(vec![("ms", ty_con(Int))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "date_day_native".to_string(),
        sig(vec![("ms", ty_con(Int))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "date_weekday_native".to_string(),
        sig(vec![("ms", ty_con(Int))], ty_con(Int), vec![]),
    );
    ctx.insert_fn(
        "date_compose_native".to_string(),
        sig(
            vec![("year", ty_con(Int)), ("month", ty_con(Int)), ("day", ty_con(Int))],
            ty_con(Int),
            vec![],
        ),
    );

    // ── std::json natives ─────────────────────────────────────────────────────

    ctx.insert_fn(
        "json_parse_value_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![Throw]),
    );
    ctx.insert_fn(
        "json_get_str_native".to_string(),
        sig(vec![("json", ty_con(Str)), ("key", ty_con(Str))], ty_con(Str), vec![Throw]),
    );
    ctx.insert_fn(
        "json_get_int_native".to_string(),
        sig(vec![("json", ty_con(Str)), ("key", ty_con(Str))], ty_con(Int), vec![Throw]),
    );
    ctx.insert_fn(
        "json_get_bool_native".to_string(),
        sig(vec![("json", ty_con(Str)), ("key", ty_con(Str))], ty_con(Bool), vec![Throw]),
    );
    ctx.insert_fn(
        "json_keys_native".to_string(),
        sig(
            vec![("json", ty_con(Str))],
            Type::List(Box::new(ty_con(Str)), Span::DUMMY),
            vec![Throw],
        ),
    );

    // ── std::log natives ──────────────────────────────────────────────────────

    ctx.insert_fn(
        "log_info_native".to_string(),
        sig(vec![("msg", ty_con(Str))], ty_con(Unit), vec![IO]),
    );
    ctx.insert_fn(
        "log_warn_native".to_string(),
        sig(vec![("msg", ty_con(Str))], ty_con(Unit), vec![IO]),
    );
    ctx.insert_fn(
        "log_error_native".to_string(),
        sig(vec![("msg", ty_con(Str))], ty_con(Unit), vec![IO]),
    );
    ctx.insert_fn(
        "log_debug_native".to_string(),
        sig(vec![("msg", ty_con(Str))], ty_con(Unit), vec![IO]),
    );

    // ── std::term natives ─────────────────────────────────────────────────────

    ctx.insert_fn(
        "term_red_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "term_green_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "term_yellow_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "term_blue_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "term_bold_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );
    ctx.insert_fn(
        "term_dim_native".to_string(),
        sig(vec![("s", ty_con(Str))], ty_con(Str), vec![]),
    );

    // ── std::yaml natives ─────────────────────────────────────────────────────

    ctx.insert_fn(
        "yaml_get_str_native".to_string(),
        sig(vec![("yaml", ty_con(Str)), ("key", ty_con(Str))], ty_con(Str), vec![Throw]),
    );
    ctx.insert_fn(
        "yaml_get_int_native".to_string(),
        sig(vec![("yaml", ty_con(Str)), ("key", ty_con(Str))], ty_con(Int), vec![Throw]),
    );
    ctx.insert_fn(
        "yaml_keys_native".to_string(),
        sig(
            vec![("yaml", ty_con(Str))],
            Type::List(Box::new(ty_con(Str)), Span::DUMMY),
            vec![Throw],
        ),
    );

    // ── std::fs natives ───────────────────────────────────────────────────────

    ctx.insert_fn(
        "fs_read_native".to_string(),
        sig(vec![("path", ty_con(Str))], ty_con(Str), vec![FS, Throw]),
    );
    ctx.insert_fn(
        "fs_write_native".to_string(),
        sig(vec![("path", ty_con(Str)), ("contents", ty_con(Str))], ty_con(Unit), vec![FS, Throw]),
    );
    ctx.insert_fn(
        "fs_append_native".to_string(),
        sig(vec![("path", ty_con(Str)), ("contents", ty_con(Str))], ty_con(Unit), vec![FS, Throw]),
    );
    ctx.insert_fn(
        "fs_exists_native".to_string(),
        sig(vec![("path", ty_con(Str))], ty_con(Bool), vec![FS]),
    );
    ctx.insert_fn(
        "fs_remove_native".to_string(),
        sig(vec![("path", ty_con(Str))], ty_con(Unit), vec![FS, Throw]),
    );
    ctx.insert_fn(
        "fs_list_dir_native".to_string(),
        sig(
            vec![("path", ty_con(Str))],
            Type::List(Box::new(ty_con(Str)), Span::DUMMY),
            vec![FS, Throw],
        ),
    );
    ctx.insert_fn(
        "fs_mkdir_all_native".to_string(),
        sig(vec![("path", ty_con(Str))], ty_con(Unit), vec![FS, Throw]),
    );

    // ── std::cache natives ────────────────────────────────────────────────────

    ctx.insert_fn(
        "cache_get_native".to_string(),
        sig(vec![("key", ty_con(Str))], ty_con(Str), vec![State]),
    );
    ctx.insert_fn(
        "cache_set_native".to_string(),
        sig(vec![("key", ty_con(Str)), ("value", ty_con(Str))], ty_con(Unit), vec![State]),
    );
    ctx.insert_fn(
        "cache_has_native".to_string(),
        sig(vec![("key", ty_con(Str))], ty_con(Bool), vec![State]),
    );
    ctx.insert_fn(
        "cache_clear_native".to_string(),
        sig(vec![], ty_con(Unit), vec![State]),
    );

    // ── std::retry natives ────────────────────────────────────────────────────

    ctx.insert_fn(
        "retry_sleep_ms_native".to_string(),
        sig(vec![("ms", ty_con(Int))], ty_con(Unit), vec![IO]),
    );
    ctx.insert_fn(
        "retry_backoff_ms_native".to_string(),
        sig(vec![("attempt", ty_con(Int))], ty_con(Int), vec![]),
    );

    // ── std::http_server natives ──────────────────────────────────────────────

    ctx.insert_fn(
        "http_serve_static_native".to_string(),
        sig(vec![("port", ty_con(Int)), ("body", ty_con(Str))], ty_con(Unit), vec![Net, IO]),
    );
    ctx.insert_fn(
        "http_get_local_native".to_string(),
        sig(vec![("port", ty_con(Int)), ("path", ty_con(Str))], ty_con(Str), vec![Net, Throw]),
    );
}
