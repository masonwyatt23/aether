//! Conversion from Aether diagnostics to LSP diagnostics.

use aether_ast::{SourceMap, Span};
use aether_types::{Diagnostic as AetherDiag, Severity};
use tower_lsp::lsp_types::{
    Diagnostic as LspDiag, DiagnosticSeverity, Position, Range,
};

/// Convert a byte offset (0-based) in `src` to an LSP `Position` (0-based line/char).
pub fn offset_to_position(src: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;
    for (i, ch) in src.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    Position { line, character }
}

/// Convert an Aether `Span` to an LSP `Range` using the source text.
pub fn span_to_range(src: &str, span: Span) -> Range {
    let start = offset_to_position(src, span.start as usize);
    let end = offset_to_position(src, span.end as usize);
    Range { start, end }
}

/// Convert an Aether `Severity` to an LSP `DiagnosticSeverity`.
pub fn severity_to_lsp(sev: Severity) -> DiagnosticSeverity {
    match sev {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Note => DiagnosticSeverity::INFORMATION,
    }
}

/// Convert a slice of Aether diagnostics to LSP diagnostics.
///
/// `map` is used to resolve the source text for each span's file.
pub fn convert_diagnostics(
    diags: &[AetherDiag],
    map: &SourceMap,
) -> Vec<LspDiag> {
    diags
        .iter()
        .map(|d| {
            let src = map.source(d.span.file);
            let range = if d.span.is_dummy() {
                // Dummy span → point to beginning of file
                Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 0 },
                }
            } else {
                span_to_range(src, d.span)
            };
            LspDiag {
                range,
                severity: Some(severity_to_lsp(d.severity)),
                message: d.msg.clone(),
                source: Some("aether".to_string()),
                ..LspDiag::default()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::{FileId, SourceMap};
    use aether_types::{Diagnostic as AetherDiag, Severity};

    #[test]
    fn error_severity_maps_to_lsp_error() {
        let sev = severity_to_lsp(Severity::Error);
        assert_eq!(sev, DiagnosticSeverity::ERROR);
    }

    #[test]
    fn warning_severity_maps_to_lsp_warning() {
        let sev = severity_to_lsp(Severity::Warning);
        assert_eq!(sev, DiagnosticSeverity::WARNING);
    }

    #[test]
    fn note_severity_maps_to_lsp_information() {
        let sev = severity_to_lsp(Severity::Note);
        assert_eq!(sev, DiagnosticSeverity::INFORMATION);
    }

    #[test]
    fn span_on_first_line_produces_correct_range() {
        // "hello world" — span covers "world" (bytes 6..11)
        let src = "hello world";
        let span = Span { file: FileId(0), start: 6, end: 11 };
        let range = span_to_range(src, span);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 11);
    }

    #[test]
    fn span_on_second_line_produces_correct_range() {
        // "abc\ndef" — span covers "def" (bytes 4..7)
        let src = "abc\ndef";
        let span = Span { file: FileId(0), start: 4, end: 7 };
        let range = span_to_range(src, span);
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, 3);
    }

    #[test]
    fn convert_diagnostics_produces_correct_lsp_diag() {
        let mut map = SourceMap::new();
        let src = "fn foo() -> Int effects {} { \"oops\" }";
        let file = map.add("test.ae", src);
        // Simulate an error at span 0..2 (covers "fn")
        let span = Span { file, start: 0, end: 2 };
        let aether_diag = AetherDiag {
            severity: Severity::Error,
            span,
            msg: "type mismatch".to_string(),
        };
        let lsp_diags = convert_diagnostics(&[aether_diag], &map);
        assert_eq!(lsp_diags.len(), 1);
        let d = &lsp_diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(d.message, "type mismatch");
        assert_eq!(d.range.start.line, 0);
        assert_eq!(d.range.start.character, 0);
        assert_eq!(d.source.as_deref(), Some("aether"));
    }

    #[test]
    fn dummy_span_maps_to_origin() {
        let map = SourceMap::new();
        let aether_diag = AetherDiag {
            severity: Severity::Warning,
            span: Span::DUMMY,
            msg: "some warning".to_string(),
        };
        let lsp_diags = convert_diagnostics(&[aether_diag], &map);
        assert_eq!(lsp_diags[0].range.start.line, 0);
        assert_eq!(lsp_diags[0].range.start.character, 0);
    }
}
