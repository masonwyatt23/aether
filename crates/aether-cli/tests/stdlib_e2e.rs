//! End-to-end tests for the three new stdlib modules: std::list, std::string, std::map.

use std::path::PathBuf;
use std::process::Command;

fn aether() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aether"))
}

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("aether_stdlib_e2e_{}_{name}", std::process::id()));
    p
}

// ── std::list ─────────────────────────────────────────────────────────────────

#[test]
fn stdlib_list_e2e() {
    let src = tmp("list_test.ae");
    std::fs::write(
        &src,
        r#"import std::list

fn main() -> Unit effects {IO, Throw} {
  let xs = [3, 1, 4, 1, 5, 9, 2]
  print(str(list_len(xs)))
  print(str(list_sum(xs)))
  print(str(list_max(xs)))
  print(str(list_contains(xs, 4)))
  print(str(list_contains(xs, 7)))
  print(str(list_count(xs, 1)))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::list check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::list run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    // list_len([3,1,4,1,5,9,2]) = 7
    assert!(stdout.contains("7"), "expected list_len=7, got: {stdout}");
    // list_sum = 25
    assert!(stdout.contains("25"), "expected list_sum=25, got: {stdout}");
    // list_max = 9
    assert!(stdout.contains("9"), "expected list_max=9, got: {stdout}");
    // list_contains 4 -> true
    assert!(stdout.contains("true"), "expected list_contains=true, got: {stdout}");
    // list_contains 7 -> false
    assert!(stdout.contains("false"), "expected list_contains=false, got: {stdout}");
    // list_count 1 -> 2
    assert!(stdout.contains("2"), "expected list_count=2, got: {stdout}");
}

#[test]
fn stdlib_list_reversed_e2e() {
    let src = tmp("list_reversed_test.ae");
    std::fs::write(
        &src,
        r#"import std::list

fn main() -> Unit effects {IO} {
  let xs = [1, 2, 3]
  let rev = list_reversed(xs)
  print(str(list_sum(rev)))
}
"#,
    )
    .unwrap();

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "list_reversed run failed:\nstderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    // sum is unchanged by reversal
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("6"), "expected sum=6 after reversal, got: {stdout}");
}

// ── std::string ───────────────────────────────────────────────────────────────

#[test]
fn stdlib_string_e2e() {
    let src = tmp("string_test.ae");
    std::fs::write(
        &src,
        r#"import std::string

fn main() -> Unit effects {IO} {
  print(str_upper("hello"))
  print(str_lower("WORLD"))
  print(str(str_contains("foobar", "oba")))
  print(str(str_starts_with("foobar", "foo")))
  print(str(str_starts_with("foobar", "bar")))
  print(str_trim("  spaces  "))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::string check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::string run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("HELLO"),  "expected HELLO, got: {stdout}");
    assert!(stdout.contains("world"),  "expected world, got: {stdout}");
    assert!(stdout.contains("true"),   "expected str_contains=true, got: {stdout}");
    assert!(stdout.contains("false"),  "expected str_starts_with=false, got: {stdout}");
    assert!(stdout.contains("spaces"), "expected trimmed 'spaces', got: {stdout}");
}

#[test]
fn stdlib_string_split_join_e2e() {
    let src = tmp("string_split_join.ae");
    std::fs::write(
        &src,
        r#"import std::string

fn main() -> Unit effects {IO} {
  let parts = str_split_on("a:b:c", ":")
  print(str_join(parts, "-"))
}
"#,
    )
    .unwrap();

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "str_split/join run failed:\nstderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("a-b-c"), "expected 'a-b-c', got: {stdout}");
}

// ── std::path ─────────────────────────────────────────────────────────────────

#[test]
fn stdlib_path_e2e() {
    let src = tmp("path_test.ae");
    std::fs::write(
        &src,
        r#"import std::path

fn main() -> Unit effects {IO, FS} {
  print(path_join("/tmp", "hello.txt"))
  print(path_basename("/home/user/file.rs"))
  print(path_dirname("/home/user/file.rs"))
  print(path_extension("archive.tar.gz"))
  print(str(path_exists("/")))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::path check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::path run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("hello.txt"),  "expected joined path, got: {stdout}");
    assert!(stdout.contains("file.rs"),    "expected basename, got: {stdout}");
    assert!(stdout.contains("user"),       "expected dirname contains 'user', got: {stdout}");
    assert!(stdout.contains("gz"),         "expected extension 'gz', got: {stdout}");
    assert!(stdout.contains("true"),       "expected path_exists('/')=true, got: {stdout}");
}

// ── std::time ─────────────────────────────────────────────────────────────────

#[test]
fn stdlib_time_e2e() {
    let src = tmp("time_test.ae");
    std::fs::write(
        &src,
        r#"import std::time

fn main() -> Unit effects {IO} {
  let ms = time_now_ms()
  print(str(ms > 0))
  let iso = time_format_iso(0)
  print(iso)
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::time check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::time run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("true"),             "expected ms>0=true, got: {stdout}");
    assert!(stdout.contains("1970-01-01T00:00:00Z"), "expected epoch ISO, got: {stdout}");
}

// ── std::env ──────────────────────────────────────────────────────────────────

#[test]
fn stdlib_env_e2e() {
    let src = tmp("env_test.ae");
    std::fs::write(
        &src,
        r#"import std::env

fn main() -> Unit effects {IO, State} {
  print(str(env_has("_AETHER_E2E_ABSENT_VAR_99")))
  env_set("_AETHER_E2E_TEST_KEY", "hello_from_aether")
  print(env_get("_AETHER_E2E_TEST_KEY"))
  print(str(env_has("_AETHER_E2E_TEST_KEY")))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::env check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::env run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("false"),               "expected env_has(absent)=false, got: {stdout}");
    assert!(stdout.contains("hello_from_aether"),   "expected env_get round-trip, got: {stdout}");
    assert!(stdout.contains("true"),                "expected env_has(set key)=true, got: {stdout}");
}

// ── std::fmt ──────────────────────────────────────────────────────────────────

#[test]
fn stdlib_fmt_e2e() {
    let src = tmp("fmt_test.ae");
    std::fs::write(
        &src,
        r#"import std::fmt

fn main() -> Unit effects {IO} {
  print(fmt1("Hello, {}!", "Aether"))
  print(fmt2("{} + {} = 3", "1", "2"))
  print(fmt3("{}-{}-{}", "a", "b", "c"))
  print(pad_left("7", 3, "0"))
  print(pad_right("hi", 5, "."))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::fmt check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::fmt run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("Hello, Aether!"), "expected fmt1 output, got: {stdout}");
    assert!(stdout.contains("1 + 2 = 3"),      "expected fmt2 output, got: {stdout}");
    assert!(stdout.contains("a-b-c"),           "expected fmt3 output, got: {stdout}");
    assert!(stdout.contains("007"),             "expected pad_left output, got: {stdout}");
    assert!(stdout.contains("hi..."),           "expected pad_right output, got: {stdout}");
}

// ── std::result ───────────────────────────────────────────────────────────────

#[test]
fn stdlib_result_e2e() {
    let src = tmp("result_test.ae");
    std::fs::write(
        &src,
        r#"import std::result

fn main() -> Unit effects {IO} {
  let ok_val = Ok("success")
  let err_val = Err("oops")
  print(str(result_is_ok(ok_val)))
  print(str(result_is_ok(err_val)))
  print(result_unwrap_or(ok_val, "fallback"))
  print(result_unwrap_or(err_val, "fallback"))
  let mapped = result_map_str(ok_val, "prefix:")
  print(result_unwrap_or(mapped, "bad"))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::result check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::result run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("true"),         "expected result_is_ok(Ok)=true, got: {stdout}");
    assert!(stdout.contains("false"),        "expected result_is_ok(Err)=false, got: {stdout}");
    assert!(stdout.contains("success"),      "expected unwrap_or(Ok)=success, got: {stdout}");
    assert!(stdout.contains("fallback"),     "expected unwrap_or(Err)=fallback, got: {stdout}");
    assert!(stdout.contains("prefix:success"), "expected result_map_str output, got: {stdout}");
}

// ── std::regex ────────────────────────────────────────────────────────────────

#[test]
fn regex_e2e() {
    let src = tmp("regex_test.ae");
    std::fs::write(
        &src,
        r#"import std::regex

fn main() -> Unit effects {IO, Throw} {
  print(str(regex_match("\\d+", "abc 42 def")))
  print(str(regex_match("\\d+", "no digits")))
  print(regex_find("\\d+", "abc 42 def"))
  print(regex_replace("\\d+", "foo 42 bar", "NUM"))
  let parts = regex_split(",", "a,b,c")
  print(str(len(parts)))
  let caps = regex_captures("(\\d{4})-(\\d{2})", "date 2024-03")
  print(str(len(caps)))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::regex check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::regex run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("true"),  "expected regex_match=true, got: {stdout}");
    assert!(stdout.contains("false"), "expected regex_match=false, got: {stdout}");
    assert!(stdout.contains("42"),    "expected regex_find=42, got: {stdout}");
    assert!(stdout.contains("NUM"),   "expected regex_replace=NUM, got: {stdout}");
    // split "a,b,c" on "," → 3 parts
    assert!(stdout.contains("3"),     "expected regex_split len=3, got: {stdout}");
    // captures 2 groups
    assert!(stdout.contains("2"),     "expected regex_captures len=2, got: {stdout}");
}

// ── std::sys ──────────────────────────────────────────────────────────────────

#[test]
fn sys_e2e() {
    let src = tmp("sys_test.ae");
    std::fs::write(
        &src,
        r#"import std::sys

fn main() -> Unit effects {IO} {
  let secs = sys_now_unix()
  print(str(secs > 0))
  let args = sys_args()
  print(str(len(args)))
  let host = sys_hostname()
  print(str(len(host) > 0))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::sys check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::sys run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    // secs > 0 = true
    assert!(stdout.contains("true"), "expected sys_now_unix>0=true, got: {stdout}");
    // hostname non-empty = true (appears twice: secs>0 and len(host)>0)
    assert!(stdout.matches("true").count() >= 2, "expected at least 2 trues, got: {stdout}");
}

// ── std::math ─────────────────────────────────────────────────────────────────

#[test]
fn math_e2e() {
    let src = tmp("math_test.ae");
    std::fs::write(
        &src,
        r#"import std::math

fn main() -> Unit effects {IO} {
  print(str(math_abs_int(-7)))
  print(str(math_pow(2, 8)))
  print(str(math_sqrt(16.0)))
  print(str(math_floor(3.9)))
  print(str(math_ceil(3.1)))
  print(str(math_round(2.5)))
  print(str(math_min_f(1.0, 2.0)))
  print(str(math_max_f(1.0, 2.0)))
  let pi = math_pi()
  print(str(pi > 3.14))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::math check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::math run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("7"),    "expected math_abs_int=7, got: {stdout}");
    assert!(stdout.contains("256"),  "expected math_pow=256, got: {stdout}");
    assert!(stdout.contains("4"),    "expected math_sqrt=4, got: {stdout}");
    assert!(stdout.contains("3"),    "expected math_floor=3, got: {stdout}");
    assert!(stdout.contains("true"), "expected pi>3.14=true, got: {stdout}");
}

// ── std::map ──────────────────────────────────────────────────────────────────

#[test]
fn stdlib_map_e2e() {
    let src = tmp("map_test.ae");
    std::fs::write(
        &src,
        r#"import std::map

fn main() -> Unit effects {IO, Throw} {
  let m = map_new()
  let m2 = map_set(m, "x", 10)
  let m3 = map_set(m2, "y", 20)
  print(str(map_has(m3, "x")))
  print(str(map_has(m3, "z")))
  print(str(map_get(m3, "x")))
  print(str(map_get(m3, "y")))
  print(str(map_size(m3)))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::map check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::map run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("true"),  "expected map_has(x)=true, got: {stdout}");
    assert!(stdout.contains("false"), "expected map_has(z)=false, got: {stdout}");
    assert!(stdout.contains("10"),    "expected map_get(x)=10, got: {stdout}");
    assert!(stdout.contains("20"),    "expected map_get(y)=20, got: {stdout}");
    assert!(stdout.contains("2"),     "expected map_size=2, got: {stdout}");
}

// ── std::base64 ───────────────────────────────────────────────────────────────

#[test]
fn stdlib_base64_e2e() {
    let src = tmp("base64_test.ae");
    std::fs::write(
        &src,
        r#"import std::base64

fn main() -> Unit effects {IO, Throw} {
  let encoded = base64_encode("hello")
  let decoded = base64_decode(encoded)
  print(encoded)
  print(decoded)
  print(str(decoded == "hello"))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(check.status.success(),
        "std::base64 check failed:\nstderr:\n{}", String::from_utf8_lossy(&check.stderr));

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(run.status.success(),
        "std::base64 run failed:\nstderr:\n{}", String::from_utf8_lossy(&run.stderr));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("aGVsbG8="), "expected base64 of 'hello': {stdout}");
    assert!(stdout.contains("hello"),    "expected decoded 'hello': {stdout}");
    assert!(stdout.contains("true"),     "expected round-trip == true: {stdout}");
}

// ── std::hash ─────────────────────────────────────────────────────────────────

#[test]
fn stdlib_hash_e2e() {
    let src = tmp("hash_test.ae");
    std::fs::write(
        &src,
        r#"import std::hash

fn main() -> Unit effects {IO} {
  let h = hash_sha256("hello")
  print(h)
  let n = hash_default("cache-key")
  print(str(n == hash_default("cache-key")))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(check.status.success(),
        "std::hash check failed:\nstderr:\n{}", String::from_utf8_lossy(&check.stderr));

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(run.status.success(),
        "std::hash run failed:\nstderr:\n{}", String::from_utf8_lossy(&run.stderr));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"),
        "expected SHA-256 of 'hello': {stdout}"
    );
    assert!(stdout.contains("true"), "expected hash_default deterministic: {stdout}");
}

// ── std::uuid ─────────────────────────────────────────────────────────────────

#[test]
fn stdlib_uuid_e2e() {
    let src = tmp("uuid_test.ae");
    std::fs::write(
        &src,
        r#"import std::uuid

fn main() -> Unit effects {IO, Rand} {
  let id = uuid_v4()
  print(str(len(id) == 36))
  let short = uuid_short()
  print(str(len(short) == 8))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(check.status.success(),
        "std::uuid check failed:\nstderr:\n{}", String::from_utf8_lossy(&check.stderr));

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(run.status.success(),
        "std::uuid run failed:\nstderr:\n{}", String::from_utf8_lossy(&run.stderr));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("true"), "expected len checks true: {stdout}");
}

// ── std::random ───────────────────────────────────────────────────────────────

#[test]
fn stdlib_random_e2e() {
    let src = tmp("random_test.ae");
    std::fs::write(
        &src,
        r#"import std::random

fn main() -> Unit effects {IO, Rand} {
  let n = random_int(1, 1)
  print(str(n == 1))
  let f = random_float()
  print(str(f >= 0.0))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(check.status.success(),
        "std::random check failed:\nstderr:\n{}", String::from_utf8_lossy(&check.stderr));

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(run.status.success(),
        "std::random run failed:\nstderr:\n{}", String::from_utf8_lossy(&run.stderr));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("true"), "expected deterministic random_int(1,1)==1: {stdout}");
}

// ── std::log ──────────────────────────────────────────────────────────────────

#[test]
fn log_e2e() {
    let src = tmp("log_test.ae");
    std::fs::write(
        &src,
        r#"import std::log

fn main() -> Unit effects {IO} {
  log_info("server started")
  log_warn("low memory")
  log_error("disk full")
  println("done")
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::log check failed:\nstderr:\n{}",
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::log run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    // stdout should only contain our explicit println, not log lines (those go to stderr)
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("done"), "expected 'done' on stdout, got: {stdout}");
    // log lines should appear on stderr
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("[INFO]"),  "expected [INFO] on stderr, got: {stderr}");
    assert!(stderr.contains("[WARN]"),  "expected [WARN] on stderr, got: {stderr}");
    assert!(stderr.contains("[ERROR]"), "expected [ERROR] on stderr, got: {stderr}");
}

// ── std::term ─────────────────────────────────────────────────────────────────

#[test]
fn term_e2e() {
    let src = tmp("term_test.ae");
    std::fs::write(
        &src,
        r#"import std::term

fn main() -> Unit effects {IO} {
  println(term_red("red"))
  println(term_green("green"))
  println(term_bold("bold"))
  println(term_clear())
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::term check failed:\nstderr:\n{}",
        String::from_utf8_lossy(&check.stderr)
    );

    // Run with NO_COLOR so output is predictable (plain strings)
    let run = aether()
        .arg("run")
        .arg(&src)
        .env("NO_COLOR", "1")
        .output()
        .expect("aether run");
    assert!(
        run.status.success(),
        "std::term run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("red"),   "expected 'red', got: {stdout}");
    assert!(stdout.contains("green"), "expected 'green', got: {stdout}");
    assert!(stdout.contains("bold"),  "expected 'bold', got: {stdout}");
}

// ── std::json (real parser) ────────────────────────────────────────────────────

#[test]
fn json_real_e2e() {
    let src = tmp("json_real_test.ae");
    std::fs::write(
        &src,
        r#"import std::json

fn main() -> Unit effects {IO, Throw} {
  let raw = "{\"name\":\"Aether\",\"version\":3,\"stable\":true}"
  let canonical = json_parse_value(raw)
  print(str(len(canonical) > 0))
  print(json_get_str(raw, "name"))
  print(str(json_get_int(raw, "version")))
  print(str(json_get_bool(raw, "stable")))
  let keys = json_keys(raw)
  print(str(len(keys)))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::json (real) check failed:\nstderr:\n{}",
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::json (real) run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("true"),   "expected canonical len>0=true, got: {stdout}");
    assert!(stdout.contains("Aether"), "expected name=Aether, got: {stdout}");
    assert!(stdout.contains("3"),      "expected version=3, got: {stdout}");
    assert!(stdout.contains("3"),      "expected 3 keys, got: {stdout}");
}

// ── std::yaml ─────────────────────────────────────────────────────────────────

#[test]
fn yaml_e2e() {
    let src = tmp("yaml_test.ae");
    // Build the YAML document string in Aether by joining lines with \n.
    // Use r##"..."## so that "# ..." inside the Aether source isn't treated
    // as a Rust raw-string terminator.
    std::fs::write(
        &src,
        r##"import std::yaml
import std::string

fn main() -> Unit effects {IO, Throw} {
  let line1 = "# Aether config"
  let line2 = "name: Aether"
  let line3 = "version: 3"
  let line4 = "author: Mason"
  let lines = [line1, line2, line3, line4]
  let doc = str_join(lines, "\n")
  print(yaml_get_str(doc, "name"))
  print(str(yaml_get_int(doc, "version")))
  let keys = yaml_keys(doc)
  print(str(len(keys)))
}
"##,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::yaml check failed:\nstderr:\n{}",
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::yaml run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("Aether"), "expected name=Aether, got: {stdout}");
    assert!(stdout.contains("3"),      "expected version=3, got: {stdout}");
    // 3 keys: name, version, author
    assert!(stdout.contains("3"),      "expected 3 keys, got: {stdout}");
}

// ── std::date ─────────────────────────────────────────────────────────────────

#[test]
fn stdlib_date_e2e() {
    let src = tmp("date_test.ae");
    std::fs::write(
        &src,
        r#"import std::date

fn main() -> Unit effects {IO} {
  print(str(date_year(0)))
  print(str(date_month(0)))
  print(str(date_day(0)))
  print(str(date_weekday(0)))
  print(str(date_compose(1970, 1, 1)))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(check.status.success(),
        "std::date check failed:\nstderr:\n{}", String::from_utf8_lossy(&check.stderr));

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(run.status.success(),
        "std::date run failed:\nstderr:\n{}", String::from_utf8_lossy(&run.stderr));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("1970"), "expected year=1970: {stdout}");
    assert!(stdout.contains("4"),    "expected weekday=4 (Thursday): {stdout}");
    // date_compose(1970,1,1) == 0 — the "0" appears on its own line
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.last().map(|l| l.trim() == "0").unwrap_or(false),
        "expected date_compose(1970,1,1)==0 on last line, got: {stdout}");
}

// ── std::fs ───────────────────────────────────────────────────────────────────

#[test]
fn fs_e2e() {
    let dir = {
        let mut p = std::env::temp_dir();
        p.push(format!("aether_fs_e2e_{}", std::process::id()));
        p
    };
    let src = tmp("fs_test.ae");

    let dir_str = dir.to_string_lossy();
    let file_path = dir.join("hello.txt");
    let file_str = file_path.to_string_lossy();

    std::fs::write(
        &src,
        format!(r#"import std::fs

fn main() -> Unit effects {{IO, FS, Throw}} {{
  fs_mkdir_all("{dir_str}")
  fs_write("{file_str}", "hello world")
  let contents = fs_read("{file_str}")
  print(contents)
  print(str(fs_exists("{file_str}")))
  let entries = fs_list_dir("{dir_str}")
  print(str(len(entries)))
  fs_remove("{file_str}")
  print(str(fs_exists("{file_str}")))
}}
"#),
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::fs check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::fs run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("hello world"), "expected file contents, got: {stdout}");
    assert!(stdout.contains("true"),  "expected exists=true after write, got: {stdout}");
    assert!(stdout.contains("false"), "expected exists=false after remove, got: {stdout}");
    // 1 file in dir
    assert!(stdout.contains("1"), "expected 1 entry in dir, got: {stdout}");

    // cleanup
    let _ = std::fs::remove_dir_all(&dir);
}

// ── std::cache ────────────────────────────────────────────────────────────────

#[test]
fn cache_e2e() {
    let src = tmp("cache_test.ae");
    std::fs::write(
        &src,
        r#"import std::cache

fn main() -> Unit effects {IO, State} {
  print(str(cache_has("k")))
  cache_set("k", "v1")
  print(str(cache_has("k")))
  print(cache_get("k"))
  cache_set("k", "v2")
  print(cache_get("k"))
  cache_clear()
  print(str(cache_has("k")))
  print(cache_get("k"))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::cache check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::cache run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    // has("k") before set -> false
    assert_eq!(lines.get(0).copied(), Some("false"), "expected false before set, got: {stdout}");
    // has("k") after set -> true
    assert_eq!(lines.get(1).copied(), Some("true"),  "expected true after set, got: {stdout}");
    // get("k") == "v1"
    assert_eq!(lines.get(2).copied(), Some("v1"),    "expected v1, got: {stdout}");
    // overwrite -> "v2"
    assert_eq!(lines.get(3).copied(), Some("v2"),    "expected v2 after overwrite, got: {stdout}");
    // has after clear -> false
    assert_eq!(lines.get(4).copied(), Some("false"), "expected false after clear, got: {stdout}");
    // get after clear -> ""
    assert_eq!(lines.get(5).copied(), Some(""),      "expected empty string after clear, got: {stdout}");
}

// ── std::retry ────────────────────────────────────────────────────────────────

#[test]
fn retry_e2e() {
    let src = tmp("retry_test.ae");
    std::fs::write(
        &src,
        r#"import std::retry

fn main() -> Unit effects {IO} {
  print(str(retry_backoff_ms(0)))
  print(str(retry_backoff_ms(1)))
  print(str(retry_backoff_ms(2)))
  print(str(retry_backoff_ms(3)))
  print(str(retry_backoff_ms(8)))
  print(str(retry_backoff_ms(9)))
  print(str(retry_attempts()))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::retry check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::retry run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.get(0).copied(), Some("100"),   "backoff(0) should be 100");
    assert_eq!(lines.get(1).copied(), Some("200"),   "backoff(1) should be 200");
    assert_eq!(lines.get(2).copied(), Some("400"),   "backoff(2) should be 400");
    assert_eq!(lines.get(3).copied(), Some("800"),   "backoff(3) should be 800");
    assert_eq!(lines.get(4).copied(), Some("25600"), "backoff(8) should be 25600");
    assert_eq!(lines.get(5).copied(), Some("30000"), "backoff(9) should be capped at 30000");
    assert_eq!(lines.get(6).copied(), Some("3"),     "retry_attempts() should be 3");
}

// ── std::http_server ──────────────────────────────────────────────────────────

#[test]
fn http_server_e2e() {
    let port: u16 = 5731;
    let body = "aether http ok";

    // Server source: serve 1 request then return.
    let server_src = tmp("http_server_test.ae");
    std::fs::write(
        &server_src,
        format!(
            r#"import std::http_server

fn main() -> Unit effects {{Net, IO}} {{
  http_serve_static({port}, "{body}")
}}
"#
        ),
    )
    .unwrap();

    let check = aether().arg("check").arg(&server_src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::http_server check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    // Client source: fetch from the server.
    let client_src = tmp("http_client_test.ae");
    std::fs::write(
        &client_src,
        format!(
            r#"import std::http_server

fn main() -> Unit effects {{IO, Net, Throw}} {{
  let resp = http_get_local({port}, "/")
  print(resp)
}}
"#
        ),
    )
    .unwrap();

    let check2 = aether().arg("check").arg(&client_src).output().expect("aether check client");
    assert!(
        check2.status.success(),
        "http_get_local check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check2.stdout),
        String::from_utf8_lossy(&check2.stderr)
    );

    // Spawn server in background thread (AETHER_SERVE_LIMIT=1 so it exits after 1 request).
    let server_src2 = server_src.clone();
    let server_handle = std::thread::spawn(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_aether"))
            .arg("run")
            .arg(&server_src2)
            .env("AETHER_SERVE_LIMIT", "1")
            .output()
            .expect("aether run server")
    });

    // Give the server time to bind.
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Run client.
    let client_out = aether()
        .arg("run")
        .arg(&client_src)
        .output()
        .expect("aether run client");

    assert!(
        client_out.status.success(),
        "http client run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&client_out.stdout),
        String::from_utf8_lossy(&client_out.stderr)
    );

    let stdout = String::from_utf8_lossy(&client_out.stdout);
    assert!(
        stdout.contains(body),
        "expected response body {body:?}, got: {stdout}"
    );

    // Wait for server to finish cleanly.
    let _ = server_handle.join();
}

// ── regex_replace_all ─────────────────────────────────────────────────────────

#[test]
fn regex_replace_all_e2e() {
    let src = tmp("regex_replace_all_test.ae");
    std::fs::write(
        &src,
        r#"import std::regex

fn main() -> Unit effects {IO, Throw} {
  print(regex_replace_all("a", "banana", "X"))
  print(regex_replace_all("\\d+", "foo 1 bar 2 baz 3", "NUM"))
  print(regex_replace_all("z", "banana", "X"))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "regex_replace_all check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "regex_replace_all run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("bXnXnX"),              "expected bXnXnX, got: {stdout}");
    assert!(stdout.contains("foo NUM bar NUM baz NUM"), "expected all digits replaced, got: {stdout}");
    assert!(stdout.contains("banana"),              "expected no-match unchanged, got: {stdout}");
}

// ── fmt arities ───────────────────────────────────────────────────────────────

#[test]
fn fmt_arities_e2e() {
    let src = tmp("fmt_arities_test.ae");
    std::fs::write(
        &src,
        r#"import std::fmt

fn main() -> Unit effects {IO} {
  print(fmt4("{} {} {} {}", "a", "b", "c", "d"))
  print(fmt5("{} {} {} {} {}", "1", "2", "3", "4", "5"))
  print(fmt_list("{} {} {}", ["x", "y", "z"]))
  print(fmt_list("{}", ["only"]))
  print(fmt_list("{} {}", ["p"]))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "fmt_arities check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "fmt_arities run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("a b c d"),   "expected fmt4 result, got: {stdout}");
    assert!(stdout.contains("1 2 3 4 5"), "expected fmt5 result, got: {stdout}");
    assert!(stdout.contains("x y z"),     "expected fmt_list 3-arg result, got: {stdout}");
    assert!(stdout.contains("only"),      "expected fmt_list single arg, got: {stdout}");
    assert!(stdout.contains("p {}"),      "expected leftover placeholder, got: {stdout}");
}

// ── std::strlist ──────────────────────────────────────────────────────────────

#[test]
fn strlist_e2e() {
    let src = tmp("strlist_test.ae");
    std::fs::write(
        &src,
        r#"import std::strlist

fn main() -> Unit effects {IO, Throw} {
  let xs = ["apple", "banana", "cherry"]
  print(str(strlist_len(xs)))
  print(strlist_get(xs, 1))
  print(strlist_join(xs, ", "))
  print(str(strlist_contains(xs, "banana")))
  print(str(strlist_contains(xs, "grape")))
  let rev = strlist_reversed(xs)
  print(strlist_head(rev))
  let tl = strlist_tail(xs)
  print(str(strlist_len(tl)))
  let single = ["only"]
  let etl = strlist_tail(single)
  print(str(strlist_len(etl)))
}
"#,
    )
    .unwrap();

    let check = aether().arg("check").arg(&src).output().expect("aether check");
    assert!(
        check.status.success(),
        "std::strlist check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = aether().arg("run").arg(&src).output().expect("aether run");
    assert!(
        run.status.success(),
        "std::strlist run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    // strlist_len = 3
    assert!(stdout.contains('3'), "expected len=3, got: {stdout}");
    // strlist_get(1) = "banana"
    assert!(stdout.contains("banana"), "expected get(1)=banana, got: {stdout}");
    // strlist_join = "apple, banana, cherry"
    assert!(stdout.contains("apple, banana, cherry"), "expected join, got: {stdout}");
    // contains "banana" -> true
    assert!(stdout.contains("true"),  "expected contains=true, got: {stdout}");
    // contains "grape" -> false
    assert!(stdout.contains("false"), "expected contains=false, got: {stdout}");
    // reversed head = "cherry"
    assert!(stdout.contains("cherry"), "expected reversed head=cherry, got: {stdout}");
    // tail of 3-elem list has len 2
    assert!(stdout.contains('2'), "expected tail len=2, got: {stdout}");
    // tail of single-element list has len 0
    assert!(stdout.contains('0'), "expected single-elem tail len=0, got: {stdout}");
}
