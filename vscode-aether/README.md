# Aether Language Support for VS Code

Syntax highlighting, snippets, and LSP integration for the [Aether](https://github.com/aether-lang/aether) language (`.ae` / `.aev` files).

## Features

- Full syntax highlighting via TextMate grammar (keywords, types, effects, agent primitives, doc comments, strings, numbers, annotations)
- Code snippets for common patterns (`fn`, `fnc`, `if`, `match`, `intro`, `spec`, `tool`, ...)
- LSP integration: diagnostics, hover, goto definition, completion — powered by `aether-lsp`

## Quickstart

### Option A — install a packaged VSIX

```bash
# 1. Build the extension
cd vscode-aether
npm install
npm run compile
npx vsce package          # produces aether-language-support-0.1.0.vsix

# 2. Install it
code --install-extension aether-language-support-0.1.0.vsix
```

### Option B — open as extension development host

1. Open the `vscode-aether/` folder in VS Code.
2. Press `F5` — a new Extension Development Host window opens with the extension loaded.

## LSP server setup

The extension expects an `aether-lsp` binary on your `PATH`. Build it from the repo:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p aether-lsp --release
# Binary at: target/release/aether-lsp
```

Then either add `target/release/` to `PATH`, or point the extension at the binary:

```jsonc
// .vscode/settings.json
{
  "aether.serverPath": "/absolute/path/to/target/release/aether-lsp"
}
```

## Settings

| Setting | Default | Description |
|---|---|---|
| `aether.serverPath` | `aether-lsp` | Path to the `aether-lsp` binary |
| `aether.trace.server` | `off` | LSP trace level: `off` / `messages` / `verbose` |

## Screenshot

_Screenshot placeholder — open an `.ae` file to see highlighting in action._

## Development

```bash
cd vscode-aether
npm install
npm run watch   # incremental TypeScript compilation
```

Requires Node ≥ 18 and VS Code ≥ 1.85.
