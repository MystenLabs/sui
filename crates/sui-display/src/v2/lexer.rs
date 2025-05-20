// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::fmt;

use move_core_types::identifier::is_valid_identifier_char;

/// Lexer for Display V2 format strings. Format strings are a mix of text and expressions.
/// Expressions are enclosed in braces and may contain multiple alternates, separated by pipes, and
/// each containing nested field, vector, or dynamic field accesses.
#[derive(Debug)]
pub(crate) struct Lexer<'s> {
    /// Remaining input to be tokenized.
    src: &'s str,

    /// The number of bytes (not characters) tokenized so far.
    off: usize,

    /// Whether the lexer is currently inside a text strand or an expression strand.
    mode: Mode,
}

#[derive(Debug)]
enum Mode {
    Text,
    Expr,
}

/// A lexeme is a token along with its offset in the source string, and the slice of source string
/// that it originated from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lexeme<'s>(pub Token, pub usize, pub &'s str);

/// Like [Lexeme] but owns the slice of source string. Useful for capturing context in an error
/// message.
#[derive(Debug)]
pub(crate) struct OwnedLexeme(Token, usize, String);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Token {
    /// '.'
    Dot,
    /// An identifier
    Ident,
    /// '{'
    LBrace,
    /// '{{'
    LLBrace,
    /// '}'
    RBrace,
    /// '}}'
    RRBrace,
    /// A strand of text.
    Text,

    /// An unexpected byte in the input string.
    Unexpected,
}

#[derive(Debug)]
pub(crate) struct TokenSet<'t>(pub &'t [Token]);

impl<'s> Lexer<'s> {
    pub(crate) fn new(src: &'s str) -> Self {
        Self {
            src,
            off: 0,
            mode: Mode::Text,
        }
    }

    /// Assuming the lexer is in text mode, return the next text token.
    fn next_text_token(&mut self) -> Option<Lexeme<'s>> {
        let bytes = self.src.as_bytes();

        use Token as T;
        Some(match bytes.first()? {
            b'{' if bytes.get(1) == Some(&b'{') => {
                self.take(T::LLBrace, 1); // discard the first brace
                self.take(T::LLBrace, 1)
            }

            b'{' => {
                self.mode = Mode::Expr;
                self.take(T::LBrace, 1)
            }

            b'}' if bytes.get(1) == Some(&b'}') => {
                self.take(T::RRBrace, 1); // discard the first brace
                self.take(T::RRBrace, 1)
            }

            // This is not a valid token within text, but recognise it so that the parser can
            // produce a better error message.
            b'}' => self.take(T::RBrace, 1),

            _ => self.take_until(T::Text, |c| ['{', '}'].contains(&c)),
        })
    }

    /// Assuming the lexer is in expression mode, return the next expression token.
    fn next_expr_token(&mut self) -> Option<Lexeme<'s>> {
        self.skip_whitespace();

        use Token as T;
        Some(match self.src.as_bytes().first()? {
            b'{' => self.take(T::LBrace, 1),

            b'}' => {
                self.mode = Mode::Text;
                self.take(T::RBrace, 1)
            }

            b'.' => self.take(T::Dot, 1),

            b'a'..=b'z' | b'A'..=b'Z' => {
                self.take_until(T::Ident, |c| !is_valid_identifier_char(c))
            }

            // If the next byte cannot be recognized, extract the next (potentially variable
            // length) character, and indicate that it is an unexpected token.
            _ => {
                let mut indices = self.src.char_indices();
                indices.next(); // skip the first character
                let bytes = indices.next().map(|i| i.0).unwrap_or(self.src.len());
                self.take(T::Unexpected, bytes)
            }
        })
    }

    fn skip_whitespace(&mut self) {
        self.take_until(Token::Text, |c: char| !c.is_whitespace());
    }

    /// Take a prefix of bytes from `self.src` until a byte satisfying pattern `p` is found, and
    /// return it as a lexeme of type `t`. If no such byte is found, take the entire remainder of
    /// the source string.
    fn take_until(&mut self, t: Token, p: impl FnMut(char) -> bool) -> Lexeme<'s> {
        self.take(t, self.src.find(p).unwrap_or(self.src.len()))
    }

    /// Take `n` bytes from the beginning of `self.src` and return them as a lexeme of type `t`.
    ///
    /// ## Safety
    ///
    /// This function assumes that there are at least `n` bytes left in `self.src`, and will panic
    /// if that is not the case.
    fn take(&mut self, t: Token, n: usize) -> Lexeme<'s> {
        let start = self.off;
        let slice = &self.src[..n];
        self.src = &self.src[n..];
        self.off += n;

        Lexeme(t, start, slice)
    }
}

impl Lexeme<'_> {
    /// Return the lexeme as an owned lexeme, with the slice of source string copied.
    pub(crate) fn detach(&self) -> OwnedLexeme {
        OwnedLexeme(self.0, self.1, self.2.to_owned())
    }
}

impl<'s> Iterator for Lexer<'s> {
    type Item = Lexeme<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        use Mode as M;
        match self.mode {
            M::Text => self.next_text_token(),
            M::Expr => self.next_expr_token(),
        }
    }
}

impl fmt::Display for OwnedLexeme {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use OwnedLexeme as L;
        use Token as T;
        match self {
            L(T::Text, _, s) => write!(f, "text {s:?}"),
            L(T::LBrace, _, _) => write!(f, "'{{'"),
            L(T::LLBrace, _, _) => write!(f, "'{{{{'"),
            L(T::RBrace, _, _) => write!(f, "'}}'"),
            L(T::RRBrace, _, _) => write!(f, "'}}}}'"),
            L(T::Ident, _, s) => write!(f, "identifier {s:?}"),
            L(T::Dot, _, _) => write!(f, "'.'"),
            L(T::Unexpected, _, s) => write!(f, "unexpected character {s:?}"),
        }?;

        write!(f, " at offset {}", self.1)
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Token as T;
        match self {
            T::Text => write!(f, "text"),
            T::LBrace => write!(f, "'{{'"),
            T::LLBrace => write!(f, "'{{{{'"),
            T::RBrace => write!(f, "'}}'"),
            T::RRBrace => write!(f, "'}}}}'"),
            T::Ident => write!(f, "an identifier"),
            T::Dot => write!(f, "'.'"),
            T::Unexpected => write!(f, "an unexpected character"),
        }
    }
}

impl fmt::Display for TokenSet<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let TokenSet(tokens) = self;

        if tokens.is_empty() {
            return write!(f, "nothing");
        }

        let (head, [tail]) = tokens.split_at(tokens.len() - 1) else {
            unreachable!("tail contains exactly one token");
        };

        if head.is_empty() {
            return write!(f, "{tail}");
        }

        let mut prefix = "one of ";
        for token in head {
            write!(f, "{prefix}{token}")?;
            prefix = ", ";
        }

        write!(f, ", or {tail}")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Lexeme as L;
    use Token as T;

    /// Simple test for a  raw literal string.
    #[test]
    fn test_all_text() {
        let lexer = Lexer::new("foo bar");
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(lexemes, vec![L(T::Text, 0, "foo bar")]);
    }

    /// Escape sequences are all text, but they will be split into multiple tokens.
    #[test]
    fn test_escapes() {
        let lexer = Lexer::new(r#"foo {{ar}}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LLBrace, 5, "{"),
                L(T::Text, 6, "ar"),
                L(T::RRBrace, 9, "}"),
            ]
        );
    }

    /// Text inside braces is tokenized as if it's an expression.
    #[test]
    fn test_expressions() {
        let lexer = Lexer::new(r#"foo {bar}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LBrace, 4, "{"),
                L(T::Ident, 5, "bar"),
                L(T::RBrace, 8, "}"),
            ],
        );
    }

    /// Expressions are tokenized to ignore whitespace.
    #[test]
    fn test_expression_whitespace() {
        let lexer = Lexer::new(r#"foo {  bar   }"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LBrace, 4, "{"),
                L(T::Ident, 7, "bar"),
                L(T::RBrace, 13, "}"),
            ],
        );
    }

    /// Field names are separated by dots in an expression.
    #[test]
    fn test_expression_dots() {
        let lexer = Lexer::new(r#"foo {bar. baz  . qux}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LBrace, 4, "{"),
                L(T::Ident, 5, "bar"),
                L(T::Dot, 8, "."),
                L(T::Ident, 10, "baz"),
                L(T::Dot, 15, "."),
                L(T::Ident, 17, "qux"),
                L(T::RBrace, 20, "}"),
            ],
        );
    }

    /// Multiple expressions test switching and back and forth between lexer modes.
    #[test]
    fn test_multiple_expressions() {
        let lexer = Lexer::new(r#"foo {bar.baz} qux {quy.quz}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LBrace, 4, "{"),
                L(T::Ident, 5, "bar"),
                L(T::Dot, 8, "."),
                L(T::Ident, 9, "baz"),
                L(T::RBrace, 12, "}"),
                L(T::Text, 13, " qux "),
                L(T::LBrace, 18, "{"),
                L(T::Ident, 19, "quy"),
                L(T::Dot, 22, "."),
                L(T::Ident, 23, "quz"),
                L(T::RBrace, 26, "}"),
            ],
        );
    }

    /// The lexer will still tokenize curlies even if they are not balanced.
    #[test]
    fn test_unbalanced_curlies() {
        let lexer = Lexer::new(r#"foo}{bar{}}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo"),
                L(T::RBrace, 3, "}"),
                L(T::LBrace, 4, "{"),
                L(T::Ident, 5, "bar"),
                L(T::LBrace, 8, "{"),
                L(T::RBrace, 9, "}"),
                L(T::RBrace, 10, "}"),
            ],
        );
    }

    /// Unexpected characters are tokenized so that the parser can produce an error.
    #[test]
    fn test_unexpected_characters() {
        let lexer = Lexer::new(r#"anything goes {@ # ! 🔥}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "anything goes "),
                L(T::LBrace, 14, "{"),
                L(T::Unexpected, 15, "@"),
                L(T::Unexpected, 17, "#"),
                L(T::Unexpected, 19, "!"),
                L(T::Unexpected, 21, "🔥"),
                L(T::RBrace, 25, "}"),
            ],
        );
    }

    // Escaped curlies shouldn't be tokenized greedily. '{{{' in text mode should be tokenized as
    // '{{' and '{', while '}}}' in expr mode should be tokenized as '}' and '}}'. This test
    // exercises these and similar cases.
    #[test]
    fn test_triple_curlies() {
        let lexer = Lexer::new(r#"foo {{{bar} {baz}}} }}} { {{ } qux"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LLBrace, 5, "{"),
                L(T::LBrace, 6, "{"),
                L(T::Ident, 7, "bar"),
                L(T::RBrace, 10, "}"),
                L(T::Text, 11, " "),
                L(T::LBrace, 12, "{"),
                L(T::Ident, 13, "baz"),
                L(T::RBrace, 16, "}"),
                L(T::RRBrace, 18, "}"),
                L(T::Text, 19, " "),
                L(T::RRBrace, 21, "}"),
                L(T::RBrace, 22, "}"),
                L(T::Text, 23, " "),
                L(T::LBrace, 24, "{"),
                L(T::LBrace, 26, "{"),
                L(T::LBrace, 27, "{"),
                L(T::RBrace, 29, "}"),
                L(T::Text, 30, " qux"),
            ],
        );
    }
}
