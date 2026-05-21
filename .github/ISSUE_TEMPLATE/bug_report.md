---
name: Bug report
about: Something in Aether is broken or behaves incorrectly
labels: bug
---

## What happened

<!-- A clear description of the bug. What did Aether do? -->

## Minimal `.ae` reproduction

<!-- The smallest Aether program that triggers the bug. Paste it in the fence below. -->

```aether
// paste here
```

## Expected behaviour

<!-- What should happen when you run the program above? -->

## Actual behaviour

<!-- What actually happens? Paste any error output, panics, or wrong values. -->

## Version info

```
# Output of: aether --version
```

**OS:** <!-- e.g. macOS 14.4, Ubuntu 24.04 -->
**Rust toolchain:** <!-- e.g. stable 1.78.0 — output of: rustc --version -->

## Affected component (if known)

<!-- Check all that apply -->
- [ ] Parser (`aether-parser`)
- [ ] Type checker / refinement solver (`aether-types`)
- [ ] Tree-walking evaluator (`aether-eval`)
- [ ] Bytecode VM (`aether-bc`)
- [ ] LSP server (`aether-lsp`)
- [ ] Standard library (`aether-stdlib`)
- [ ] CLI (`aether-cli`)
- [ ] Other / unsure
