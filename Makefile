# Aether project Makefile. Mirrors `Justfile` for users without `just` installed.
# Run `make help` to see what's available.

.PHONY: help build test lint fmt fmt-check demo install clean docker docs

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-15s %s\n", $$1, $$2}'

build:                  ## Release build of every crate
	cargo build --release --workspace

test:                   ## Run every workspace test
	cargo test --workspace

lint:                   ## Clippy with the workspace lints
	cargo clippy --workspace --all-targets -- -A clippy::pedantic

fmt:                    ## rustfmt every file in place
	cargo fmt --all

fmt-check:              ## Check formatting (used by CI)
	cargo fmt --all -- --check

demo:                   ## Run every example program
	@for f in examples/*.ae; do \
	  echo "── $$f ──"; \
	  cargo run -q -p aether-cli -- run $$f || true; \
	  echo; \
	done

install:                ## Install the `aether` binary into ~/.cargo/bin
	cargo install --path crates/aether-cli

docker:                 ## Build the container image (aether:0.3)
	docker build -t aether:0.3 .

docs:                   ## Generate static HTML docs into ./docs-site/
	cargo run --release -q -p aether-cli -- docgen examples -o docs-site --title "Aether v0.3"

clean:                  ## cargo clean
	cargo clean

prepush: fmt-check test lint   ## Pre-push sweep: fmt + tests + clippy
	@echo "✓ ready to push"
