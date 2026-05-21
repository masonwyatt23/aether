//! Aether lexer.
//!
//! One tokenizer accepts both the compact (`.ae`) and verbose (`.aev`) forms.
//! Keywords are a strict superset; operator tokens are deliberately small and
//! BPE-friendly. Comments start with `#` and run to end of line. Docstrings
//! are `##` runs that bind to the next declaration (handled by the parser).

use aether_ast::span::Span;
use aether_ast::FileId;
use logos::Logos;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")]
pub enum Tok {
    // Literals
    #[regex(r"-?[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<i64>().ok())]
    Int(i64),

    #[regex(r"-?[0-9][0-9_]*\.[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<f64>().ok())]
    Float(f64),

    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        unescape_str(&s[1..s.len()-1])
    })]
    Str(String),

    #[token("true", |_| true)]
    #[token("false", |_| false)]
    Bool(bool),

    // Identifiers (must come after keywords below; logos handles via priority)
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", |lex| lex.slice().to_string(), priority = 2)]
    Ident(String),

    // Keywords (shared between compact + verbose)
    #[token("fn")]
    Fn,
    #[token("let")]
    Let,
    #[token("in")]
    In,
    #[token("if")]
    If,
    #[token("then")]
    Then,
    #[token("else")]
    Else,
    #[token("match")]
    Match,
    #[token("with")]
    With,
    #[token("type")]
    Type,
    #[token("import")]
    Import,
    #[token("from")]
    From,
    #[token("as")]
    As,
    #[token("module")]
    Module,
    #[token("effects")]
    Effects,
    #[token("effect")]
    Effect,
    #[token("where")]
    Where,
    #[token("ensuring")]
    Ensuring,
    #[token("requires")]
    Requires,
    #[token("ensures")]
    Ensures,
    #[token("spec")]
    Spec,
    #[token("tool")]
    Tool,
    #[token("introspect")]
    Introspect,
    #[token("summarize")]
    Summarize,
    #[token("provenance")]
    Provenance,
    #[token("confident")]
    Confident,
    #[token("assume")]
    Assume,
    #[token("confidence")]
    Confidence,
    #[token("result")]
    Result_,
    #[token("not")]
    NotKw,
    #[token("and")]
    AndKw,
    #[token("or")]
    OrKw,
    #[token("do")]
    Do,

    // Annotations
    #[token("@no_prov")]
    AtNoProv,
    #[token("@pure")]
    AtPure,
    #[token("@inline")]
    AtInline,

    // Punctuation & operators
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(",")]
    Comma,
    #[token(";")]
    Semi,
    #[token(":")]
    Colon,
    #[token("::")]
    ColonColon,
    #[token(".")]
    Dot,
    #[token("?")]
    Question,
    #[token("~")]
    Tilde,
    #[token("|")]
    Pipe,
    #[token("|>")]
    PipeForward,
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("=")]
    Eq,
    #[token(":=")]
    Walrus,
    #[token("==")]
    EqEq,
    #[token("!=")]
    BangEq,
    #[token("<")]
    Lt,
    #[token("<=")]
    Le,
    #[token(">")]
    Gt,
    #[token(">=")]
    Ge,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("++")]
    PlusPlus,
    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,
    #[token("!")]
    Bang,
    #[token("&")]
    Amp,
    #[token("??")]
    QuestionQuestion,

    // Doc string lines starting with `##`. Captured intact (without the leading `## `).
    #[regex(r"##[^\n]*", |lex| {
        let s = lex.slice();
        s.trim_start_matches('#').trim_start().to_string()
    })]
    DocLine(String),

    // Plain comments (skipped).
    #[regex(r"#[^\n]*", logos::skip)]
    Comment,
}

fn unescape_str(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                '0' => out.push('\0'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

#[derive(Debug, thiserror::Error)]
pub enum LexError {
    #[error("unexpected character at byte {start}-{end}: {snippet:?}")]
    Unexpected {
        start: u32,
        end: u32,
        snippet: String,
    },
}

/// Tokenize an entire source string. Skips comments and whitespace.
pub fn lex(file: FileId, source: &str) -> Result<Vec<Token>, LexError> {
    let mut lex = Tok::lexer(source);
    let mut out = Vec::new();
    while let Some(res) = lex.next() {
        let r = lex.span();
        match res {
            Ok(tok) => out.push(Token {
                tok,
                span: Span::new(file, r),
            }),
            Err(()) => {
                return Err(LexError::Unexpected {
                    start: r.start as u32,
                    end: r.end as u32,
                    snippet: source[r].to_string(),
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(src: &str) -> Vec<Tok> {
        lex(FileId(0), src)
            .unwrap()
            .into_iter()
            .map(|t| t.tok)
            .collect()
    }

    #[test]
    fn integers_floats_bools() {
        assert_eq!(
            ts("0 1 -2 3.14 true false"),
            vec![
                Tok::Int(0),
                Tok::Int(1),
                Tok::Int(-2),
                Tok::Float(3.14),
                Tok::Bool(true),
                Tok::Bool(false),
            ]
        );
    }

    #[test]
    fn identifiers_and_keywords() {
        let toks = ts("fn add x y let if introspect provenance");
        assert!(matches!(toks[0], Tok::Fn));
        assert!(matches!(toks[1], Tok::Ident(ref s) if s == "add"));
        assert!(matches!(toks[4], Tok::Let));
        assert!(matches!(toks[5], Tok::If));
        assert!(matches!(toks[6], Tok::Introspect));
        assert!(matches!(toks[7], Tok::Provenance));
    }

    #[test]
    fn operators_and_punct() {
        let toks = ts("(a,b) -> c !{} = x+y");
        // (, ident, comma, ident, ), arrow, ident, bang, {, }, =, ident, +, ident
        assert!(matches!(toks[0], Tok::LParen));
        assert!(matches!(toks[5], Tok::Arrow));
        assert!(matches!(toks[7], Tok::Bang));
        assert!(matches!(toks[10], Tok::Eq));
        assert!(matches!(toks[12], Tok::Plus));
    }

    #[test]
    fn refinement_compact() {
        let toks = ts("n:I{n>=0}");
        // ident, :, ident, {, ident, >=, int, }
        assert!(matches!(toks[0], Tok::Ident(ref s) if s == "n"));
        assert!(matches!(toks[1], Tok::Colon));
        assert!(matches!(toks[2], Tok::Ident(ref s) if s == "I"));
        assert!(matches!(toks[3], Tok::LBrace));
        assert!(matches!(toks[5], Tok::Ge));
        assert!(matches!(toks[6], Tok::Int(0)));
        assert!(matches!(toks[7], Tok::RBrace));
    }

    #[test]
    fn string_with_escapes() {
        let toks = ts(r#""hello\nworld""#);
        assert!(matches!(&toks[0], Tok::Str(s) if s == "hello\nworld"));
    }

    #[test]
    fn comment_skipped() {
        let toks = ts("a # comment\nb");
        assert_eq!(toks.len(), 2);
    }

    #[test]
    fn doc_line_kept() {
        let toks = ts("## docstring here\nfn f");
        assert!(matches!(&toks[0], Tok::DocLine(s) if s == "docstring here"));
        assert!(matches!(toks[1], Tok::Fn));
    }
}
