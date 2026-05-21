# aether-lsp

Language Server Protocol server for the [Aether](../../README.md) language.

## Building

```bash
cargo build -p aether-lsp --release
# Binary at: target/release/aether-lsp
```

## Features

| Feature | Status |
|---|---|
| Diagnostics on open/save | Real — re-runs parser + type/effect checker |
| Hover (identifier type) | Real — walks AST, resolves via TypeCtx |
| Goto definition | Real — resolves to declaration span |
| Completion | Real — lists fns, builtins, lets, tools, type aliases |

## Editor setup

### VS Code

Install the [generic LSP client extension](https://marketplace.visualstudio.com/items?itemName=llvm-vs-code-extensions.vscode-clangd) or add to `.vscode/settings.json`:

```jsonc
{
  "aether-lsp.serverPath": "/path/to/target/release/aether-lsp"
}
```

Or use the official Aether extension (when published) which bundles this binary.

Manual `settings.json` approach via `vscode-languageclient`:

```json
{
  "[aether]": {},
  "languageServerExample.maxNumberOfProblems": 100
}
```

For now, use the **"Language Server Protocol Client"** (`lsp-client`) approach:

```json
{
  "languageserver": {
    "aether": {
      "command": "/path/to/aether-lsp",
      "args": [],
      "filetypes": ["ae", "aether"]
    }
  }
}
```

### Neovim (nvim-lspconfig)

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.aether then
  configs.aether = {
    default_config = {
      cmd = { '/path/to/aether-lsp' },
      filetypes = { 'aether' },
      root_dir = lspconfig.util.root_pattern('Cargo.toml', '.git'),
      settings = {},
    },
  }
end

lspconfig.aether.setup {}
```

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "aether"
scope = "source.aether"
file-types = ["ae"]
language-servers = ["aether-lsp"]

[language-server.aether-lsp]
command = "/path/to/aether-lsp"
```

## Transport

The server communicates over **stdin/stdout** using the standard JSON-RPC LSP protocol. No TCP port is used.
