// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::fmt;

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
    /// '@'
    At,
    /// ':'
    Colon,
    /// '::'
    CColon,
    /// ','
    Comma,
    /// '.'
    Dot,
    /// An identifier
    Ident,
    /// '<'
    LAngle,
    /// '{'
    LBrace,
    /// '['
    LBracket,
    /// '{{'
    LLBrace,
    /// '('
    LParen,
    /// A decimal number, optionally separated by underscores.
    NumDec,
    /// A hexadecimal number, prefixed with '0x' (not included in the span), optionally separated
    /// by underscores.
    NumHex,
    /// '|'
    Pipe,
    /// '#'
    Pound,
    /// '>'
    RAngle,
    /// '}'
    RBrace,
    /// ']'
    RBracket,
    /// ')'
    RParen,
    /// '}}'
    RRBrace,
    /// Strings are surrounded by single quotes. Quotes and backslashes inside strings are escaped
    /// with backslashes.
    String,
    /// A strand of text.
    Text,

    /// An unexpected byte in the input string.
    Unexpected,

    /// Whitespace around expressions.
    Whitespace,
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
                self.advance(1);
                self.take(T::LLBrace, 1)
            }

            b'{' => {
                self.mode = Mode::Expr;
                self.take(T::LBrace, 1)
            }

            b'}' if bytes.get(1) == Some(&b'}') => {
                self.advance(1);
                self.take(T::RRBrace, 1)
            }

            // This is not a valid token within text, but recognise it so that the parser can
            // produce a better error message.
            b'}' => self.take(T::RBrace, 1),

            _ => self.take_until(T::Text, |b| b"{}".contains(&b)),
        })
    }

    /// Assuming the lexer is in expression mode, return the next expression token.
    fn next_expr_token(&mut self) -> Option<Lexeme<'s>> {
        let bytes = self.src.as_bytes();

        use Token as T;
        Some(match bytes.first()? {
            b'@' => self.take(T::At, 1),

            b':' if bytes.get(1) == Some(&b':') => self.take(T::CColon, 2),

            b':' => self.take(T::Colon, 1),

            b',' => self.take(T::Comma, 1),

            b'.' => self.take(T::Dot, 1),

            b'0' if bytes.get(1) == Some(&b'x')
                && bytes.get(2).is_some_and(|b| is_valid_hex_byte(*b)) =>
            {
                self.advance(2);
                self.take_until(T::NumHex, |c| !is_valid_hex_byte(c))
            }

            b'0'..=b'9' => self.take_until(T::NumDec, |c| !is_valid_decimal_byte(c)),

            b'a'..=b'z' | b'A'..=b'Z' => {
                self.take_until(T::Ident, |c| !is_valid_identifier_byte(c))
            }

            b'<' => self.take(T::LAngle, 1),

            b'{' => self.take(T::LBrace, 1),

            b'[' => self.take(T::LBracket, 1),

            b'(' => self.take(T::LParen, 1),

            b'|' => self.take(T::Pipe, 1),

            b'#' => self.take(T::Pound, 1),

            b'>' => self.take(T::RAngle, 1),

            b'}' => {
                self.mode = Mode::Text;
                self.take(T::RBrace, 1)
            }

            b']' => self.take(T::RBracket, 1),

            b')' => self.take(T::RParen, 1),

            b'\'' => {
                // Set the escaped indicator to true initially so we don't interpret the starting
                // quote as an ending quote.
                let mut escaped = true;
                for (i, b) in self.src.bytes().enumerate() {
                    if escaped {
                        escaped = false;
                    } else if b == b'\\' {
                        escaped = true;
                    } else if b == b'\'' {
                        self.advance(1);
                        let content = self.take(T::String, i - 1);
                        self.advance(1);
                        return Some(content);
                    }
                }

                // Reached the end of the byte stream and didn't find a closing quote -- treat the
                // partial string as an unexpected token.
                self.take(T::Unexpected, self.src.len())
            }

            // Explicitly tokenize whitespace.
            _ if self.src.chars().next().is_some_and(char::is_whitespace) => self.take(
                Token::Whitespace,
                self.src
                    .find(|c: char| !c.is_whitespace())
                    .unwrap_or(self.src.len()),
            ),

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

    /// Take a prefix of bytes from `self.src` until a byte satisfying pattern `p` is found, and
    /// return it as a lexeme of type `t`. If no such byte is found, take the entire remainder of
    /// the source string.
    fn take_until(&mut self, t: Token, p: impl FnMut(u8) -> bool) -> Lexeme<'s> {
        self.take(t, self.src.bytes().position(p).unwrap_or(self.src.len()))
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
        self.advance(n);

        Lexeme(t, start, slice)
    }

    /// Move the cursor forward by `n` bytes.
    ///
    /// ## Safety
    ///
    /// This function assumes that `n` is less than or equal to the length of `self.src`, and will
    /// panic if that is not the case.
    fn advance(&mut self, n: usize) {
        self.src = &self.src[n..];
        self.off += n;
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
            L(T::At, _, _) => write!(f, "'@'"),
            L(T::Colon, _, _) => write!(f, "':'"),
            L(T::CColon, _, _) => write!(f, "'::'"),
            L(T::Comma, _, _) => write!(f, "','"),
            L(T::Dot, _, _) => write!(f, "'.'"),
            L(T::Ident, _, s) => write!(f, "identifier {s:?}"),
            L(T::LAngle, _, _) => write!(f, "'<'"),
            L(T::LBrace, _, _) => write!(f, "'{{'"),
            L(T::LBracket, _, _) => write!(f, "'['"),
            L(T::LLBrace, _, _) => write!(f, "'{{{{'"),
            L(T::LParen, _, _) => write!(f, "'('"),
            L(T::NumDec, _, s) => write!(f, "decimal number {s:?}"),
            L(T::NumHex, _, s) => write!(f, "hexadecimal number {s:?}"),
            L(T::Pipe, _, _) => write!(f, "'|'"),
            L(T::Pound, _, _) => write!(f, "'#'"),
            L(T::RAngle, _, _) => write!(f, "'>'"),
            L(T::RBrace, _, _) => write!(f, "'}}'"),
            L(T::RBracket, _, _) => write!(f, "']'"),
            L(T::RParen, _, _) => write!(f, "')'"),
            L(T::RRBrace, _, _) => write!(f, "'}}}}'"),
            L(T::String, _, s) => write!(f, "string {s:?}"),
            L(T::Text, _, s) => write!(f, "text {s:?}"),
            L(T::Unexpected, _, s) => write!(f, "unexpected {s:?}"),
            L(T::Whitespace, _, _) => write!(f, "whitespace"),
        }?;

        write!(f, " at offset {}", self.1)
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Token as T;
        match self {
            T::At => write!(f, "'@'"),
            T::Colon => write!(f, "':'"),
            T::CColon => write!(f, "'::'"),
            T::Comma => write!(f, "','"),
            T::Dot => write!(f, "'.'"),
            T::Ident => write!(f, "an identifier"),
            T::LAngle => write!(f, "'<'"),
            T::LBrace => write!(f, "'{{'"),
            T::LBracket => write!(f, "'['"),
            T::LLBrace => write!(f, "'{{{{'"),
            T::LParen => write!(f, "'('"),
            T::NumDec => write!(f, "a decimal number"),
            T::NumHex => write!(f, "a hexadecimal number"),
            T::Pipe => write!(f, "'|'"),
            T::Pound => write!(f, "'#'"),
            T::RAngle => write!(f, "'>'"),
            T::RBrace => write!(f, "'}}'"),
            T::RBracket => write!(f, "']'"),
            T::RParen => write!(f, "')'"),
            T::RRBrace => write!(f, "'}}}}'"),
            T::String => write!(f, "a string"),
            T::Text => write!(f, "text"),
            T::Unexpected => write!(f, "an unexpected character"),
            T::Whitespace => write!(f, "whitespace"),
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

fn is_valid_identifier_byte(b: u8) -> bool {
    matches!(b, b'_' | b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9')
}

fn is_valid_hex_byte(b: u8) -> bool {
    matches!(b, b'_' | b'a'..=b'f' | b'A'..=b'F' | b'0'..=b'9')
}

fn is_valid_decimal_byte(b: u8) -> bool {
    matches!(b, b'_' | b'0'..=b'9')
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
                L(T::Whitespace, 5, "  "),
                L(T::Ident, 7, "bar"),
                L(T::Whitespace, 10, "   "),
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
                L(T::Whitespace, 9, " "),
                L(T::Ident, 10, "baz"),
                L(T::Whitespace, 13, "  "),
                L(T::Dot, 15, "."),
                L(T::Whitespace, 16, " "),
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
        let lexer = Lexer::new(r#"anything goes {? % ! 🔥}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "anything goes "),
                L(T::LBrace, 14, "{"),
                L(T::Unexpected, 15, "?"),
                L(T::Whitespace, 16, " "),
                L(T::Unexpected, 17, "%"),
                L(T::Whitespace, 18, " "),
                L(T::Unexpected, 19, "!"),
                L(T::Whitespace, 20, " "),
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
                L(T::Whitespace, 25, " "),
                L(T::LBrace, 26, "{"),
                L(T::LBrace, 27, "{"),
                L(T::Whitespace, 28, " "),
                L(T::RBrace, 29, "}"),
                L(T::Text, 30, " qux"),
            ],
        );
    }

    /// Pipes separate top-level expressions, but are only parsed inside expressions, not inside
    /// text.
    #[test]
    fn test_alternates() {
        let lexer = Lexer::new(r#"foo | {bar | baz.qux} | quy"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo | "),
                L(T::LBrace, 6, "{"),
                L(T::Ident, 7, "bar"),
                L(T::Whitespace, 10, " "),
                L(T::Pipe, 11, "|"),
                L(T::Whitespace, 12, " "),
                L(T::Ident, 13, "baz"),
                L(T::Dot, 16, "."),
                L(T::Ident, 17, "qux"),
                L(T::RBrace, 20, "}"),
                L(T::Text, 21, " | quy"),
            ],
        );
    }

    // Display supports two kinds of index -- `foo[i]` and `bar[[j]]`. Unlike braces, doubly nested
    // brackets do not have their own token. The two cases are distinguished by the parser, which
    // uses significant whitespace to distinguish between two separate `]`'s vs a single `]]`.
    #[test]
    fn test_indices() {
        let lexer = Lexer::new(r#"foo {bar[baz].qux[[quy]][quz]}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LBrace, 4, "{"),
                L(T::Ident, 5, "bar"),
                L(T::LBracket, 8, "["),
                L(T::Ident, 9, "baz"),
                L(T::RBracket, 12, "]"),
                L(T::Dot, 13, "."),
                L(T::Ident, 14, "qux"),
                L(T::LBracket, 17, "["),
                L(T::LBracket, 18, "["),
                L(T::Ident, 19, "quy"),
                L(T::RBracket, 22, "]"),
                L(T::RBracket, 23, "]"),
                L(T::LBracket, 24, "["),
                L(T::Ident, 25, "quz"),
                L(T::RBracket, 28, "]"),
                L(T::RBrace, 29, "}"),
            ],
        );
    }

    /// Numbers can be represented in decimal or hexadecimal (prefixed with 0x).
    #[test]
    fn test_numeric_literals() {
        let lexer = Lexer::new(r#"{123 0x123 def 0xdef}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::NumDec, 1, "123"),
                L(T::Whitespace, 4, " "),
                L(T::NumHex, 7, "123"),
                L(T::Whitespace, 10, " "),
                L(T::Ident, 11, "def"),
                L(T::Whitespace, 14, " "),
                L(T::NumHex, 17, "def"),
                L(T::RBrace, 20, "}"),
            ],
        );
    }

    /// Numbers can optionally be grouped using underscores. Underscores cannot be trailing, but
    /// otherwise can appear in every position
    #[test]
    fn test_numeric_literal_underscores() {
        let lexer = Lexer::new(r#"{123_456 0x12_ab_de _123}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::NumDec, 1, "123_456"),
                L(T::Whitespace, 8, " "),
                L(T::NumHex, 11, "12_ab_de"),
                L(T::Whitespace, 19, " "),
                L(T::Unexpected, 20, "_"),
                L(T::NumDec, 21, "123"),
                L(T::RBrace, 24, "}"),
            ],
        );
    }

    /// Address literals are numbers prefixed with '@' -- typically, they are hexadecimal numbers
    /// but both kinds are supported.
    #[test]
    fn test_address_literals() {
        let lexer = Lexer::new(r#"{@123 @0x123}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::At, 1, "@"),
                L(T::NumDec, 2, "123"),
                L(T::Whitespace, 5, " "),
                L(T::At, 6, "@"),
                L(T::NumHex, 9, "123"),
                L(T::RBrace, 12, "}"),
            ],
        );
    }

    /// If the hexadecimal token is incomplete, it is not recognised as a number.
    #[test]
    fn test_incomplete_hexadecimal() {
        let lexer = Lexer::new(r#"{0x}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::NumDec, 1, "0"),
                L(T::Ident, 2, "x"),
                L(T::RBrace, 3, "}"),
            ],
        );
    }

    /// Vector literals are always prefixed by the 'vector' keyword. Empty vectors must specify a
    /// type parameter (which is optional for non-empty vectors).
    #[test]
    fn test_vector_literals() {
        let lexer = Lexer::new(r#"{vector[1, 2, 3] vector<u32> vector[4u64]}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::Ident, 1, "vector"),
                L(T::LBracket, 7, "["),
                L(T::NumDec, 8, "1"),
                L(T::Comma, 9, ","),
                L(T::Whitespace, 10, " "),
                L(T::NumDec, 11, "2"),
                L(T::Comma, 12, ","),
                L(T::Whitespace, 13, " "),
                L(T::NumDec, 14, "3"),
                L(T::RBracket, 15, "]"),
                L(T::Whitespace, 16, " "),
                L(T::Ident, 17, "vector"),
                L(T::LAngle, 23, "<"),
                L(T::Ident, 24, "u32"),
                L(T::RAngle, 27, ">"),
                L(T::Whitespace, 28, " "),
                L(T::Ident, 29, "vector"),
                L(T::LBracket, 35, "["),
                L(T::NumDec, 36, "4"),
                L(T::Ident, 37, "u64"),
                L(T::RBracket, 40, "]"),
                L(T::RBrace, 41, "}"),
            ],
        );
    }

    /// Struct types are fully-qualified, with a numerical (hexadecimal) address.
    #[test]
    fn test_types() {
        let lexer = Lexer::new(r#"{0x2::table::Table<address, 0x2::coin::Coin<0x2::sui::SUI>>}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::NumHex, 3, "2"),
                L(T::CColon, 4, "::"),
                L(T::Ident, 6, "table"),
                L(T::CColon, 11, "::"),
                L(T::Ident, 13, "Table"),
                L(T::LAngle, 18, "<"),
                L(T::Ident, 19, "address"),
                L(T::Comma, 26, ","),
                L(T::Whitespace, 27, " "),
                L(T::NumHex, 30, "2"),
                L(T::CColon, 31, "::"),
                L(T::Ident, 33, "coin"),
                L(T::CColon, 37, "::"),
                L(T::Ident, 39, "Coin"),
                L(T::LAngle, 43, "<"),
                L(T::NumHex, 46, "2"),
                L(T::CColon, 47, "::"),
                L(T::Ident, 49, "sui"),
                L(T::CColon, 52, "::"),
                L(T::Ident, 54, "SUI"),
                L(T::RAngle, 57, ">"),
                L(T::RAngle, 58, ">"),
                L(T::RBrace, 59, "}"),
            ],
        );
    }

    /// A positional struct literal is a struct type followed by its (positional) fields, separated
    /// by commas, surrounded by parentheses.
    #[test]
    fn test_positional_struct_literals() {
        let lexer = Lexer::new(r#"{0x2::balance::Balance<0x2::sui::SUI>(42u64)}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::NumHex, 3, "2"),
                L(T::CColon, 4, "::"),
                L(T::Ident, 6, "balance"),
                L(T::CColon, 13, "::"),
                L(T::Ident, 15, "Balance"),
                L(T::LAngle, 22, "<"),
                L(T::NumHex, 25, "2"),
                L(T::CColon, 26, "::"),
                L(T::Ident, 28, "sui"),
                L(T::CColon, 31, "::"),
                L(T::Ident, 33, "SUI"),
                L(T::RAngle, 36, ">"),
                L(T::LParen, 37, "("),
                L(T::NumDec, 38, "42"),
                L(T::Ident, 40, "u64"),
                L(T::RParen, 43, ")"),
                L(T::RBrace, 44, "}"),
            ],
        );
    }

    /// Struct literals can also include field names -- these are purely informational, they don't
    /// affect the encoded output.
    #[test]
    fn test_struct_literals() {
        let lexer = Lexer::new(r#"{0x2::coin::Coin<0x2::sui::SUI> { id: @0x123, value: 42u64 }}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::NumHex, 3, "2"),
                L(T::CColon, 4, "::"),
                L(T::Ident, 6, "coin"),
                L(T::CColon, 10, "::"),
                L(T::Ident, 12, "Coin"),
                L(T::LAngle, 16, "<"),
                L(T::NumHex, 19, "2"),
                L(T::CColon, 20, "::"),
                L(T::Ident, 22, "sui"),
                L(T::CColon, 25, "::"),
                L(T::Ident, 27, "SUI"),
                L(T::RAngle, 30, ">"),
                L(T::Whitespace, 31, " "),
                L(T::LBrace, 32, "{"),
                L(T::Whitespace, 33, " "),
                L(T::Ident, 34, "id"),
                L(T::Colon, 36, ":"),
                L(T::Whitespace, 37, " "),
                L(T::At, 38, "@"),
                L(T::NumHex, 41, "123"),
                L(T::Comma, 44, ","),
                L(T::Whitespace, 45, " "),
                L(T::Ident, 46, "value"),
                L(T::Colon, 51, ":"),
                L(T::Whitespace, 52, " "),
                L(T::NumDec, 53, "42"),
                L(T::Ident, 55, "u64"),
                L(T::Whitespace, 58, " "),
                L(T::RBrace, 59, "}"),
                L(T::RBrace, 60, "}"),
            ],
        );
    }

    /// Enums are like structs but with an additional variant component. The variant must at least
    /// specify the variant index, and can optionally specify a variant name, which is only
    /// relevant for documentation purposes (it does not affect the encoding).
    #[test]
    fn test_enum_literals() {
        let lexer =
            Lexer::new(r#"{0x2::option::Option<u64>::1(42) 0x2::option::Option<u64>::Some#1(43)}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::NumHex, 3, "2"),
                L(T::CColon, 4, "::"),
                L(T::Ident, 6, "option"),
                L(T::CColon, 12, "::"),
                L(T::Ident, 14, "Option"),
                L(T::LAngle, 20, "<"),
                L(T::Ident, 21, "u64"),
                L(T::RAngle, 24, ">"),
                L(T::CColon, 25, "::"),
                L(T::NumDec, 27, "1"),
                L(T::LParen, 28, "("),
                L(T::NumDec, 29, "42"),
                L(T::RParen, 31, ")"),
                L(T::Whitespace, 32, " "),
                L(T::NumHex, 35, "2"),
                L(T::CColon, 36, "::"),
                L(T::Ident, 38, "option"),
                L(T::CColon, 44, "::"),
                L(T::Ident, 46, "Option"),
                L(T::LAngle, 52, "<"),
                L(T::Ident, 53, "u64"),
                L(T::RAngle, 56, ">"),
                L(T::CColon, 57, "::"),
                L(T::Ident, 59, "Some"),
                L(T::Pound, 63, "#"),
                L(T::NumDec, 64, "1"),
                L(T::LParen, 65, "("),
                L(T::NumDec, 66, "43"),
                L(T::RParen, 68, ")"),
                L(T::RBrace, 69, "}"),
            ],
        );
    }

    /// Tokenizing three kinds of string literals hex, binary, and regular.
    #[test]
    fn string_literals() {
        let lexer = Lexer::new(r#"{x'0f00' b'bar' 'baz'}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::Ident, 1, "x"),
                L(T::String, 3, "0f00"),
                L(T::Whitespace, 8, " "),
                L(T::Ident, 9, "b"),
                L(T::String, 11, "bar"),
                L(T::Whitespace, 15, " "),
                L(T::String, 17, "baz"),
                L(T::RBrace, 21, "}"),
            ],
        );
    }

    /// Make sure the string does not stop early on an escaped quote, it's fine to escape random
    /// characters, and an escaped backslash does not eat the closing quote.
    #[test]
    fn test_string_literal_escapes() {
        let lexer = Lexer::new(r#"{'\' \x \\'}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LBrace, 0, "{"),
                L(T::String, 2, r#"\' \x \\"#),
                L(T::RBrace, 11, "}"),
            ],
        );
    }

    /// If the string literal is not closed, the whole sequence is treated as an "unexpected"
    /// token.
    #[test]
    fn test_string_literal_trailing() {
        let lexer = Lexer::new(r#"{'foo bar}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![L(T::LBrace, 0, "{"), L(T::Unexpected, 1, "'foo bar}"),]
        );
    }
}
