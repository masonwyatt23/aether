# Contributing to Aether

## Setup

```sh
# 1. Build the entire workspace
cargo build --workspace

# 2. Run the test suite (must stay green)
cargo test --workspace

# 3. Run lints
just lint
# equivalent to: cargo clippy --workspace --all-targets
```

Requires Rust ≥ 1.75 (set in `Cargo.toml` via `rust-version`).

## Coding Style

- Format with `cargo fmt --all` before committing (enforced in CI).
- No warnings allowed: `cargo clippy --workspace --all-targets -D warnings`
  must pass cleanly. The workspace suppresses a handful of pedantic lints
  (see `[workspace.lints.clippy]` in the root `Cargo.toml`) — don't add new
  per-crate `#[allow(...)]` without discussion.
- Prefer descriptive names over abbreviations; keep functions under ~60 lines.

## Pull Request Checklist

```
Title:       <imperative short summary, ≤ 72 chars>
Motivation:  Why is this change needed? Link related issues.
Test plan:   Which tests cover it? Did you add new ones?
Screenshot:  (If the change is visible in the REPL or playground, attach one.)
```

- Target the `main` branch.
- Keep PRs focused — one logical change per PR.
- Run `just prepush` (`fmt` + `test` + `lint`) locally before opening.

## References

- **`Justfile`** — all dev commands with descriptions (`just --list`).
- **`spec/TOUR.md`** — language tour and design rationale; read this before
  working on the parser, type checker, or evaluator.
- **`crates/`** — each crate has its own `Cargo.toml`; see the workspace root
  `Cargo.toml` for shared dependencies and lint configuration.
