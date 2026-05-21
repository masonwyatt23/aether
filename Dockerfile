# Aether CLI in a container.
#
# Build:   docker build -t aether:0.3 .
# Run:     docker run --rm -v "$PWD:/work" aether:0.3 check /work/main.ae
# REPL:    docker run --rm -it aether:0.3 repl

FROM rust:1.94-slim AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
# wasm + lsp are not part of the slim image
RUN cargo build --release \
    -p aether-cli \
    -p aether-lsp

# ── runtime ──────────────────────────────────────────────────────────────────

FROM debian:bookworm-slim

# CA certs for the optional `--network` LLM tool
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/aether     /usr/local/bin/aether
COPY --from=builder /src/target/release/aether-lsp /usr/local/bin/aether-lsp

WORKDIR /work
ENV AETHER_MEM_PATH=/work/.aether/mem.json

ENTRYPOINT ["aether"]
CMD ["--help"]
