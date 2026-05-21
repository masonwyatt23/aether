//! Source spans and file maps.

use std::ops::Range;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FileId(pub u32);

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const DUMMY: Span = Span {
        file: FileId(u32::MAX),
        start: 0,
        end: 0,
    };

    #[must_use]
    pub fn new(file: FileId, range: Range<usize>) -> Self {
        Span {
            file,
            start: range.start as u32,
            end: range.end as u32,
        }
    }

    #[must_use]
    pub fn join(self, other: Span) -> Span {
        debug_assert_eq!(self.file, other.file, "cannot join spans across files");
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    #[must_use]
    pub fn is_dummy(self) -> bool {
        self.file.0 == u32::MAX
    }
}

/// A source map records the file contents loaded for a compilation so spans
/// can be resolved back to source text. Compilers and diagnostics share one.
#[derive(Debug, Default, Clone)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub name: String,
    pub source: String,
}

impl SourceMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: impl Into<String>, source: impl Into<String>) -> FileId {
        let id = FileId(u32::try_from(self.files.len()).expect("too many source files"));
        self.files.push(SourceFile {
            name: name.into(),
            source: source.into(),
        });
        id
    }

    #[must_use]
    pub fn get(&self, file: FileId) -> Option<&SourceFile> {
        self.files.get(file.0 as usize)
    }

    #[must_use]
    pub fn source(&self, file: FileId) -> &str {
        self.get(file).map_or("", |f| f.source.as_str())
    }

    #[must_use]
    pub fn name(&self, file: FileId) -> &str {
        self.get(file).map_or("<unknown>", |f| f.name.as_str())
    }

    /// Resolve a span to a `(line, col)` 1-based location of its start.
    #[must_use]
    pub fn line_col(&self, span: Span) -> (usize, usize) {
        let src = self.source(span.file);
        let mut line = 1usize;
        let mut col = 1usize;
        for (i, ch) in src.char_indices() {
            if i >= span.start as usize {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    #[must_use]
    pub fn slice(&self, span: Span) -> &str {
        let src = self.source(span.file);
        let r = span.range();
        if r.end <= src.len() {
            &src[r]
        } else {
            ""
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_takes_outer_bounds() {
        let f = FileId(0);
        let a = Span::new(f, 2..5);
        let b = Span::new(f, 4..9);
        let j = a.join(b);
        assert_eq!(j.start, 2);
        assert_eq!(j.end, 9);
    }

    #[test]
    fn line_col_resolves() {
        let mut m = SourceMap::new();
        let f = m.add("t.ae", "abc\ndef\nghi");
        let span = Span::new(f, 5..6);
        assert_eq!(m.line_col(span), (2, 2));
    }
}
