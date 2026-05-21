# Security Policy

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Use GitHub's private vulnerability reporting: go to the **Security** tab of
this repository and click **"Report a vulnerability"**. This keeps the details
confidential until a fix is released.

Include in your report:
- A description of the vulnerability and the affected component
- Reproduction steps or a minimal test case
- Your assessment of impact and severity

We will acknowledge receipt within **48 hours** and aim to provide a mitigation
or fix within **14 days** for critical issues, **30 days** for moderate ones.

## Supported Versions

| Version | Supported |
|---------|-----------|
| v0.3.x  | Yes       |
| < v0.3  | No        |

## Scope

The following components are **in scope**:

| Component | Crate |
|-----------|-------|
| Parser | `aether-parser` |
| Type checker / refinement solver | `aether-types` |
| Tree-walking evaluator | `aether-eval` |
| Bytecode compiler & VM | `aether-bc` |

## Out of Scope

- **Stub LLM responses** — the built-in tool stubs return canned data and do
  not make real network calls; sandbox escapes through them are not applicable.
- **WASM playground sandbox boundary** — the playground runs in the browser's
  own sandbox; browser-level escapes are the browser vendor's responsibility.
- Denial-of-service via untrusted Aether source fed to the REPL (expected
  behaviour; programs should be treated as untrusted input regardless).

## Disclosure

We follow responsible disclosure. Once a fix is released we will publish a
brief advisory in `CHANGELOG.md` and credit the reporter unless anonymity
is requested.
