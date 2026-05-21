//! Aether Language Server — library crate.
//!
//! # Architecture
//!
//! ```text
//! main.rs          thin binary: set up tokio + stdio transport, call lib::run()
//! lib.rs           AetherLsp server struct + LanguageServer impl
//! diag.rs          Aether Diagnostic → LSP Diagnostic conversion (pure, tested)
//! symbols.rs       hover / goto-definition / completion helpers (pure, tested)
//! ```
//!
//! # Editor setup
//!
//! See `crates/aether-lsp/README.md` for VS Code / Neovim configuration.

pub mod diag;
pub mod symbols;

use std::collections::HashMap;
use std::sync::Arc;

use aether_ast::SourceMap;
use aether_parser::parse_module;
use aether_types::check_module;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use diag::convert_diagnostics;
use symbols::{completion_names, find_name_at, resolve_symbol};

// ---------------------------------------------------------------------------
// Per-document state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DocState {
    text: String,
    #[allow(dead_code)]
    version: i32,
}

// ---------------------------------------------------------------------------
// Server struct
// ---------------------------------------------------------------------------

pub struct AetherLsp {
    client: Client,
    docs: Arc<RwLock<HashMap<Url, DocState>>>,
}

impl AetherLsp {
    pub fn new(client: Client) -> Self {
        AetherLsp { client, docs: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Parse + type-check `src` and push diagnostics to the client.
    async fn check_and_publish(&self, uri: Url, version: Option<i32>, src: &str) {
        let mut map = SourceMap::new();
        let file_id = map.add(uri.as_str(), src);

        // Parse
        let (lsp_diags, parse_ok) = match parse_module(file_id, src) {
            Err(e) => {
                // Convert parser error to a single diagnostic
                let span = e.span().unwrap_or(aether_ast::Span {
                    file: file_id,
                    start: 0,
                    end: src.len() as u32,
                });
                let lsp_d = Diagnostic {
                    range: diag::span_to_range(src, span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("parse error: {e:?}"),
                    source: Some("aether".to_string()),
                    ..Diagnostic::default()
                };
                (vec![lsp_d], false)
            }
            Ok(module) => {
                let (_ctx, aether_diags) = check_module(&module);
                (convert_diagnostics(&aether_diags, &map), true)
            }
        };

        let _ = parse_ok; // suppress unused warning
        self.client
            .publish_diagnostics(uri, lsp_diags, version)
            .await;
    }

    /// Resolve a hover / goto request at a given LSP position in `uri`.
    fn resolve_at(
        &self,
        uri: &Url,
        src: &str,
        position: Position,
    ) -> Option<symbols::SymbolInfo> {
        let mut map = SourceMap::new();
        let file_id = map.add(uri.as_str(), src);
        let module = parse_module(file_id, src).ok()?;
        let (ctx, _) = check_module(&module);

        // Convert LSP position → byte offset
        let offset = position_to_offset(src, position);
        let name = find_name_at(&module, offset as u32)?;
        resolve_symbol(&module, &ctx, &name)
    }

    /// Build a completion list for `uri` at `position`.
    fn completions_at(&self, uri: &Url, src: &str) -> Vec<CompletionItem> {
        let mut map = SourceMap::new();
        let file_id = map.add(uri.as_str(), src);
        let module = match parse_module(file_id, src) {
            Ok(m) => m,
            Err(_) => return vec![],
        };
        let (ctx, _) = check_module(&module);
        let pairs = completion_names(&module, &ctx);
        pairs
            .into_iter()
            .map(|(label, detail)| CompletionItem {
                label,
                detail: Some(detail),
                kind: Some(CompletionItemKind::FUNCTION),
                ..CompletionItem::default()
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Utility: LSP position → byte offset
// ---------------------------------------------------------------------------

fn position_to_offset(src: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in src.char_indices() {
        if line == pos.line && col == pos.character {
            return i;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    src.len()
}

// ---------------------------------------------------------------------------
// LanguageServer implementation
// ---------------------------------------------------------------------------

#[tower_lsp::async_trait]
impl LanguageServer for AetherLsp {
    async fn initialize(&self, _params: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "aether-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    ..CompletionOptions::default()
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "aether-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    // --- Document lifecycle ---

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        let version = params.text_document.version;
        {
            let mut docs = self.docs.write().await;
            docs.insert(uri.clone(), DocState { text: text.clone(), version });
        }
        self.check_and_publish(uri, Some(version), &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let version = params.text_document.version;
        // We use FULL sync — take the last change.
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text;
            {
                let mut docs = self.docs.write().await;
                docs.insert(uri.clone(), DocState { text: text.clone(), version });
            }
            self.check_and_publish(uri, Some(version), &text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        // Re-check on save; grab current text from store.
        let text = {
            let docs = self.docs.read().await;
            docs.get(&uri).map(|d| d.text.clone())
        };
        if let Some(src) = text {
            self.check_and_publish(uri, None, &src).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.docs.write().await.remove(&uri);
        // Clear diagnostics
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    // --- Hover ---

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let src = {
            let docs = self.docs.read().await;
            docs.get(uri).map(|d| d.text.clone())
        };
        let src = match src {
            Some(s) => s,
            None => return Ok(None),
        };
        let info = self.resolve_at(uri, &src, pos);
        Ok(info.map(|i| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: i.hover_text(),
            }),
            range: None,
        }))
    }

    // --- Goto definition ---

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let src = {
            let docs = self.docs.read().await;
            docs.get(uri).map(|d| d.text.clone())
        };
        let src = match src {
            Some(s) => s,
            None => return Ok(None),
        };
        let info = match self.resolve_at(uri, &src, pos) {
            Some(i) => i,
            None => return Ok(None),
        };
        if info.def_span.is_dummy() {
            return Ok(None);
        }
        let def_range = diag::span_to_range(&src, info.def_span);
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: def_range,
        })))
    }

    // --- Completion ---

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> LspResult<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let src = {
            let docs = self.docs.read().await;
            docs.get(uri).map(|d| d.text.clone())
        };
        let src = match src {
            Some(s) => s,
            None => return Ok(None),
        };
        let items = self.completions_at(uri, &src);
        Ok(Some(CompletionResponse::Array(items)))
    }
}

// ---------------------------------------------------------------------------
// Server entry point
// ---------------------------------------------------------------------------

/// Start the LSP server on stdin/stdout (the standard transport for editors).
pub async fn run() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = tower_lsp::LspService::new(AetherLsp::new);
    tower_lsp::Server::new(stdin, stdout, socket).serve(service).await;
}

// ---------------------------------------------------------------------------
// Integration tests (no running LSP client needed)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::{SourceMap as ASTSourceMap, Span};
    use aether_types::{Diagnostic as AetherDiag, Severity};

    // Helper: parse + check a snippet and get aether diagnostics
    fn check_snippet(src: &str) -> Vec<AetherDiag> {
        let mut map = ASTSourceMap::new();
        let file = map.add("test.ae", src);
        match parse_module(file, src) {
            Ok(module) => {
                let (_ctx, diags) = check_module(&module);
                diags
            }
            Err(e) => {
                let span = e.span().unwrap_or(Span { file, start: 0, end: src.len() as u32 });
                vec![AetherDiag {
                    severity: Severity::Error,
                    span,
                    msg: format!("{e:?}"),
                }]
            }
        }
    }

    #[test]
    fn effect_error_produces_diagnostic() {
        // net_fetch declares Net effect but fn only allows empty effects — should error
        let src =
            "fn fetch(url: Str) -> Str effects {} { http_get(url) }";
        let diags = check_snippet(src);
        assert!(
            diags.iter().any(|d| d.severity == Severity::Error),
            "expected at least one error, got: {diags:?}"
        );
    }

    #[test]
    fn well_typed_fn_has_no_errors() {
        let src = "fn add(x: Int, y: Int) -> Int effects {} { x + y }";
        let diags = check_snippet(src);
        let errors: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn diagnostic_conversion_roundtrip() {
        let mut map = ASTSourceMap::new();
        let src = "fn foo() -> Int effects {} { true }";
        let file = map.add("foo.ae", src);
        let aether_diag = AetherDiag {
            severity: Severity::Error,
            span: Span { file, start: 3, end: 6 },
            msg: "type mismatch: expected Int, got Bool".to_string(),
        };
        let lsp_diags = diag::convert_diagnostics(&[aether_diag], &map);
        assert_eq!(lsp_diags.len(), 1);
        let d = &lsp_diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(d.message, "type mismatch: expected Int, got Bool");
        // "foo" starts at offset 3, which is line 0 char 3
        assert_eq!(d.range.start.line, 0);
        assert_eq!(d.range.start.character, 3);
        assert_eq!(d.range.end.character, 6);
    }
}
