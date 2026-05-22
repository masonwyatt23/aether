//! End-to-end test for `aether serve` — the persistent check server.

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn serve_handles_a_batch_of_requests() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aether"))
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn `aether serve`");

    // One verified program, one refuted, one unparseable — newline-delimited
    // JSON requests. The `\n` sequences are JSON newline escapes inside the
    // `source` string.
    let verified = "{\"id\":\"a\",\"source\":\"fn f(x: Int) -> Int\\n  \
        where result >= x\\n  effects {} {\\n  x\\n}\\n\"}\n";
    let refuted = "{\"id\":2,\"source\":\"fn f(x: Int) -> Int\\n  \
        where result > x\\n  effects {} {\\n  x\\n}\\n\"}\n";
    let unparseable = "{\"id\":\"c\",\"source\":\"fn f(x: Int) ->\"}\n";

    {
        let mut stdin = child.stdin.take().expect("stdin");
        for req in [verified, refuted, unparseable] {
            stdin.write_all(req.as_bytes()).expect("write request");
        }
        // stdin dropped here -> the serve loop sees EOF and exits.
    }

    let out = child.wait_with_output().expect("wait for `aether serve`");
    assert!(out.status.success(), "serve exited non-zero");
    let text = String::from_utf8(out.stdout).expect("utf-8 stdout");
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();

    assert_eq!(lines.len(), 3, "expected 3 responses, got:\n{text}");
    assert!(
        lines[0].contains("\"id\":\"a\"") && lines[0].contains("\"errors\": 0"),
        "verified response wrong: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("\"id\":2") && lines[1].contains("\"errors\": 1"),
        "refuted response wrong: {}",
        lines[1]
    );
    assert!(
        lines[2].contains("\"id\":\"c\"") && lines[2].contains("\"errors\": 1"),
        "parse-error response wrong: {}",
        lines[2]
    );
}
