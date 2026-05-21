## Aether dev commands. Install `just` (https://github.com/casey/just) and run `just <cmd>`.

# Show the catalog
default:
    @just --list

# Build the whole workspace in release mode
build:
    cargo build --release --workspace

# Run every test in the workspace
test:
    cargo test --workspace

# Run clippy with the workspace's lint profile
lint:
    cargo clippy --workspace --all-targets

# Format every Rust source file
fmt:
    cargo fmt --all

# Demo: run every example program and show the output
demo:
    @for f in examples/*.ae; do \
      echo "── $$f ──"; \
      cargo run -q -p aether-cli -- run $$f || true; \
      echo ""; \
    done

# Run the in-language test suite over example programs that contain tests
ae-test FILE="examples/09_test_blocks.ae":
    cargo run -q -p aether-cli -- test {{FILE}}

# Run the in-language benchmark suite
ae-bench FILE="examples/14_benchmarks.ae" ITERS="200":
    cargo run -q -p aether-cli -- bench {{FILE}} --iters {{ITERS}}

# Open a REPL session
repl:
    cargo run -q -p aether-cli -- repl

# Type-check + run with network tools enabled (needs ANTHROPIC_API_KEY for LLM)
run-net FILE:
    cargo run -q -p aether-cli -- --network run {{FILE}}

# Watch a file and re-check on every change
watch FILE:
    cargo run -q -p aether-cli -- watch {{FILE}}

# Emit machine-readable JSON AST
ast-json FILE:
    cargo run -q -p aether-cli -- ast {{FILE}} --json --pretty

# Run the LSP server (point your editor at this command)
lsp:
    cargo run -q -p aether-lsp

# Build the VS Code extension package
vscode:
    cd vscode-aether && npm install && npm run compile

# Run the bytecode VM benchmarks
bc-bench:
    cargo bench -p aether-bc

# A full "before-PR" sweep
prepush: fmt test lint
    @echo "✓ ready to push"
