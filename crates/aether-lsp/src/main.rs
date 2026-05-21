//! `aether-lsp` — Language Server Protocol server for the Aether language.
//!
//! Run this binary and point your editor at it. It communicates over stdio.
//!
//! # Quick start
//!
//! ```bash
//! cargo build -p aether-lsp --release
//! # Then configure your editor — see crates/aether-lsp/README.md
//! ```

#[tokio::main]
async fn main() {
    aether_lsp::run().await;
}
