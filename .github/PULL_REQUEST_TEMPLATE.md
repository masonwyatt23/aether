## Summary

<!-- What does this PR do? One paragraph is fine. -->

## Motivation

<!-- Why is this change needed? Link the issue(s) this resolves. -->

Closes #<!-- issue number -->

## Test plan

- [ ] `cargo test --workspace` passes locally
- [ ] `cargo clippy --workspace --all-targets -- -D warnings -A clippy::pedantic` passes
- [ ] `cargo fmt --all -- --check` passes (`just prepush` runs all three)
- [ ] New tests added for new behaviour (or explain why existing tests are sufficient)
- [ ] Snapshot tests updated if output changed (`aether snap`)

## Breaking changes

- [ ] This PR changes a public API, language syntax, bytecode format, or stdlib module signature
  <!-- If checked, describe what breaks and what the migration path is. -->

## Notes for reviewers

<!-- Anything that needs special attention: performance impact, tricky edge cases, deferred follow-ups. -->
