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

    /// Nesting of curly braces. At level 0, the lexer is in text mode. At all other levels, it is
    /// in expression mode.
    level: usize,
}

/// A lexeme is a slice of the source string marked with a token. The `bool` field indicates
/// whether the lexeme was preceded by whitespace or not.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lexeme<'s>(pub bool, pub Token, pub usize, pub &'s str);

/// Like [Lexeme] but owns the slice of source string. Useful for capturing context in an error
/// message.
#[derive(Debug)]
pub(crate) struct OwnedLexeme(pub bool, pub Token, pub usize, pub String);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
}

impl<'s> Lexer<'s> {
    pub(crate) fn new(src: &'s str) -> Self {
        Self {
            src,
            off: 0,
            level: 0,
        }
    }

    /// Assuming the lexer is in text mode, return the next text token.
    fn next_text_token(&mut self) -> Option<Lexeme<'s>> {
        let bytes = self.src.as_bytes();

        use Token as T;
        Some(match bytes.first()? {
            b'{' if bytes.get(1) == Some(&b'{') => {
                self.advance(1);
                self.take(false, T::LLBrace, 1)
            }

            b'{' => {
                self.level += 1;
                self.take(false, T::LBrace, 1)
            }

            b'}' if bytes.get(1) == Some(&b'}') => {
                self.advance(1);
                self.take(false, T::RRBrace, 1)
            }

            // This is not a valid token within text, but is recognised so that the parser can
            // produce a better error message. `level` is not decremenetd because we should already
            // been in text mode, meaning the level is already 0, and a decrement would underflow
            // it.
            b'}' => self.take(false, T::RBrace, 1),

            _ => self.take_until(false, T::Text, |b| b"{}".contains(&b)),
        })
    }

    /// Assuming the lexer is in expression mode, return the next expression token.
    fn next_expr_token(&mut self) -> Option<Lexeme<'s>> {
        let ws = self.take_whitespace();
        let bytes = self.src.as_bytes();

        use Token as T;
        Some(match bytes.first()? {
            b'@' => self.take(ws, T::At, 1),

            b':' if bytes.get(1) == Some(&b':') => self.take(ws, T::CColon, 2),

            b':' => self.take(ws, T::Colon, 1),

            b',' => self.take(ws, T::Comma, 1),

            b'.' => self.take(ws, T::Dot, 1),

            b'0' if bytes.get(1) == Some(&b'x')
                && bytes.get(2).is_some_and(|b| is_valid_hex_byte(*b)) =>
            {
                self.advance(2);
                self.take_until(ws, T::NumHex, |c| !is_valid_hex_byte(c))
            }

            b'0'..=b'9' => self.take_until(ws, T::NumDec, |c| !is_valid_decimal_byte(c)),

            b'a'..=b'z' | b'A'..=b'Z' => {
                self.take_until(ws, T::Ident, |c| !is_valid_identifier_byte(c))
            }

            b'<' => self.take(ws, T::LAngle, 1),

            b'{' => {
                self.level += 1;
                self.take(ws, T::LBrace, 1)
            }

            b'[' => self.take(ws, T::LBracket, 1),

            b'(' => self.take(ws, T::LParen, 1),

            b'|' => self.take(ws, T::Pipe, 1),

            b'#' => self.take(ws, T::Pound, 1),

            b'>' => self.take(ws, T::RAngle, 1),

            b'}' => {
                self.level -= 1;
                self.take(ws, T::RBrace, 1)
            }

            b']' => self.take(ws, T::RBracket, 1),

            b')' => self.take(ws, T::RParen, 1),

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
                        let content = self.take(ws, T::String, i - 1);
                        self.advance(1);
                        return Some(content);
                    }
                }

                // Reached the end of the byte stream and didn't find a closing quote -- treat the
                // partial string as an unexpected token.
                self.take(ws, T::Unexpected, self.src.len())
            }

            // If the next byte cannot be recognized, extract the next (potentially variable
            // length) character, and indicate that it is an unexpected token.
            _ => {
                let mut indices = self.src.char_indices();
                indices.next(); // skip the first character
                let bytes = indices.next().map(|i| i.0).unwrap_or(self.src.len());
                self.take(ws, T::Unexpected, bytes)
            }
        })
    }

    fn take_whitespace(&mut self) -> bool {
        let Lexeme(_, _, _, slice) = self.take(
            false,
            Token::Unexpected,
            self.src
                .find(|c: char| !c.is_whitespace())
                .unwrap_or(self.src.len()),
        );

        !slice.is_empty()
    }

    /// Take a prefix of bytes from `self.src` until a byte satisfying pattern `p` is found, and
    /// return it as a lexeme of type `t`. If no such byte is found, take the entire remainder of
    /// the source string.
    fn take_until(&mut self, ws: bool, t: Token, p: impl FnMut(u8) -> bool) -> Lexeme<'s> {
        let n = self.src.bytes().position(p).unwrap_or(self.src.len());
        self.take(ws, t, n)
    }

    /// Take `n` bytes from the beginning of `self.src` and return them as a lexeme of type `t`.
    ///
    /// ## Safety
    ///
    /// This function assumes that there are at least `n` bytes left in `self.src`, and will panic
    /// if that is not the case.
    fn take(&mut self, ws: bool, t: Token, n: usize) -> Lexeme<'s> {
        let start = self.off;
        let slice = &self.src[..n];
        self.advance(n);

        Lexeme(ws, t, start, slice)
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
        OwnedLexeme(self.0, self.1, self.2, self.3.to_owned())
    }
}

impl<'s> Iterator for Lexer<'s> {
    type Item = Lexeme<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.level == 0 {
            self.next_text_token()
        } else {
            self.next_expr_token()
        }
    }
}

impl fmt::Display for OwnedLexeme {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use OwnedLexeme as L;
        use Token as T;

        if self.0 {
            write!(f, "whitespace followed by ")?;
        }

        match self {
            L(_, T::At, _, _) => write!(f, "'@'"),
            L(_, T::Colon, _, _) => write!(f, "':'"),
            L(_, T::CColon, _, _) => write!(f, "'::'"),
            L(_, T::Comma, _, _) => write!(f, "','"),
            L(_, T::Dot, _, _) => write!(f, "'.'"),
            L(_, T::Ident, _, s) => write!(f, "identifier {s:?}"),
            L(_, T::LAngle, _, _) => write!(f, "'<'"),
            L(_, T::LBrace, _, _) => write!(f, "'{{'"),
            L(_, T::LBracket, _, _) => write!(f, "'['"),
            L(_, T::LLBrace, _, _) => write!(f, "'{{{{'"),
            L(_, T::LParen, _, _) => write!(f, "'('"),
            L(_, T::NumDec, _, s) => write!(f, "decimal number {s:?}"),
            L(_, T::NumHex, _, s) => write!(f, "hexadecimal number {s:?}"),
            L(_, T::Pipe, _, _) => write!(f, "'|'"),
            L(_, T::Pound, _, _) => write!(f, "'#'"),
            L(_, T::RAngle, _, _) => write!(f, "'>'"),
            L(_, T::RBrace, _, _) => write!(f, "'}}'"),
            L(_, T::RBracket, _, _) => write!(f, "']'"),
            L(_, T::RParen, _, _) => write!(f, "')'"),
            L(_, T::RRBrace, _, _) => write!(f, "'}}}}'"),
            L(_, T::String, _, s) => write!(f, "string {s:?}"),
            L(_, T::Text, _, s) => write!(f, "text {s:?}"),
            L(_, T::Unexpected, _, s) => write!(f, "{s:?}"),
        }?;

        write!(f, " at offset {}", self.2)
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
            T::Unexpected => write!(f, "unexpected input"),
        }
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
    use insta::assert_snapshot;
    use Lexeme as L;

    fn lexemes(src: &str) -> String {
        Lexer::new(src)
            .map(|L(ws, t, o, s)| format!("L({ws:?}, {t:?}, {o:?}, {s:?})"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Simple test for a  raw literal string.
    #[test]
    fn test_all_text() {
        assert_snapshot!(lexemes("foo bar"), @r###"L(false, Text, 0, "foo bar")"###);
    }

    /// Escape sequences are all text, but they will be split into multiple tokens.
    #[test]
    fn test_escapes() {
        assert_snapshot!(lexemes(r#"foo {{bar}}"#), @r###"
        L(false, Text, 0, "foo ")
        L(false, LLBrace, 5, "{")
        L(false, Text, 6, "bar")
        L(false, RRBrace, 10, "}")
        "###);
    }

    /// Text inside braces is tokenized as if it's an expression.
    #[test]
    fn test_expressions() {
        assert_snapshot!(lexemes(r#"foo {bar}"#), @r###"
        L(false, Text, 0, "foo ")
        L(false, LBrace, 4, "{")
        L(false, Ident, 5, "bar")
        L(false, RBrace, 8, "}")
        "###);
    }

    /// Expressions are tokenized to ignore whitespace.
    #[test]
    fn test_expression_whitespace() {
        assert_snapshot!(lexemes(r#"foo {  bar   }"#), @r###"
        L(false, Text, 0, "foo ")
        L(false, LBrace, 4, "{")
        L(true, Ident, 7, "bar")
        L(true, RBrace, 13, "}")
        "###);
    }

    /// Field names are separated by dots in an expression.
    #[test]
    fn test_expression_dots() {
        assert_snapshot!(lexemes(r#"foo {bar. baz  . qux}"#), @r###"
        L(false, Text, 0, "foo ")
        L(false, LBrace, 4, "{")
        L(false, Ident, 5, "bar")
        L(false, Dot, 8, ".")
        L(true, Ident, 10, "baz")
        L(true, Dot, 15, ".")
        L(true, Ident, 17, "qux")
        L(false, RBrace, 20, "}")
        "###);
    }

    /// Multiple expressions test switching and back and forth between lexer modes.
    #[test]
    fn test_multiple_expressions() {
        assert_snapshot!(lexemes(r#"foo {bar.baz} qux {quy.quz}"#), @r###"
        L(false, Text, 0, "foo ")
        L(false, LBrace, 4, "{")
        L(false, Ident, 5, "bar")
        L(false, Dot, 8, ".")
        L(false, Ident, 9, "baz")
        L(false, RBrace, 12, "}")
        L(false, Text, 13, " qux ")
        L(false, LBrace, 18, "{")
        L(false, Ident, 19, "quy")
        L(false, Dot, 22, ".")
        L(false, Ident, 23, "quz")
        L(false, RBrace, 26, "}")
        "###);
    }

    /// Expressions can include nested curly braces. Meeting the first well-bracketed closing curly
    /// brace should not cause the lexer to exit expression mode.
    #[test]
    fn test_nested_curlies() {
        assert_snapshot!(lexemes(r#"foo {bar {baz} qux}"#), @r###"
        L(false, Text, 0, "foo ")
        L(false, LBrace, 4, "{")
        L(false, Ident, 5, "bar")
        L(true, LBrace, 9, "{")
        L(false, Ident, 10, "baz")
        L(false, RBrace, 13, "}")
        L(true, Ident, 15, "qux")
        L(false, RBrace, 18, "}")
        "###);
    }

    /// The lexer will still tokenize curlies even if they are not balanced.
    #[test]
    fn test_unbalanced_curlies() {
        assert_snapshot!(lexemes(r#"foo}{bar{}}"#), @r###"
        L(false, Text, 0, "foo")
        L(false, RBrace, 3, "}")
        L(false, LBrace, 4, "{")
        L(false, Ident, 5, "bar")
        L(false, LBrace, 8, "{")
        L(false, RBrace, 9, "}")
        L(false, RBrace, 10, "}")
        "###);
    }

    /// Unexpected characters are tokenized so that the parser can produce an error.
    #[test]
    fn test_unexpected_characters() {
        assert_snapshot!(lexemes(r#"anything goes {? % ! 🔥}"#), @r###"
        L(false, Text, 0, "anything goes ")
        L(false, LBrace, 14, "{")
        L(false, Unexpected, 15, "?")
        L(true, Unexpected, 17, "%")
        L(true, Unexpected, 19, "!")
        L(true, Unexpected, 21, "🔥")
        L(false, RBrace, 25, "}")
        "###);
    }

    // Escaped curlies shouldn't be tokenized greedily. '{{{' in text mode should be tokenized as
    // '{{' and '{', while '}}}' in expr mode should be tokenized as '}' and '}}'. This test
    // exercises these and similar cases.
    #[test]
    fn test_triple_curlies() {
        assert_snapshot!(lexemes(r#"foo {{{bar} {baz}}} }}} { {{ } qux"#), @r###"
        L(false, Text, 0, "foo ")
        L(false, LLBrace, 5, "{")
        L(false, LBrace, 6, "{")
        L(false, Ident, 7, "bar")
        L(false, RBrace, 10, "}")
        L(false, Text, 11, " ")
        L(false, LBrace, 12, "{")
        L(false, Ident, 13, "baz")
        L(false, RBrace, 16, "}")
        L(false, RRBrace, 18, "}")
        L(false, Text, 19, " ")
        L(false, RRBrace, 21, "}")
        L(false, RBrace, 22, "}")
        L(false, Text, 23, " ")
        L(false, LBrace, 24, "{")
        L(true, LBrace, 26, "{")
        L(false, LBrace, 27, "{")
        L(true, RBrace, 29, "}")
        L(true, Ident, 31, "qux")
        "###);
    }

    /// Pipes separate top-level expressions, but are only parsed inside expressions, not inside
    /// text.
    #[test]
    fn test_alternates() {
        assert_snapshot!(lexemes(r#"foo | {bar | baz.qux} | quy"#), @r###"
        L(false, Text, 0, "foo | ")
        L(false, LBrace, 6, "{")
        L(false, Ident, 7, "bar")
        L(true, Pipe, 11, "|")
        L(true, Ident, 13, "baz")
        L(false, Dot, 16, ".")
        L(false, Ident, 17, "qux")
        L(false, RBrace, 20, "}")
        L(false, Text, 21, " | quy")
        "###);
    }

    // Display supports two kinds of index -- `foo[i]` and `bar[[j]]`. Unlike braces, doubly nested
    // brackets do not have their own token. The two cases are distinguished by the parser, which
    // uses significant whitespace to distinguish between two separate `]`'s vs a single `]]`.
    #[test]
    fn test_indices() {
        assert_snapshot!(lexemes(r#"foo {bar[baz].qux[[quy]][quz]}"#), @r###"
        L(false, Text, 0, "foo ")
        L(false, LBrace, 4, "{")
        L(false, Ident, 5, "bar")
        L(false, LBracket, 8, "[")
        L(false, Ident, 9, "baz")
        L(false, RBracket, 12, "]")
        L(false, Dot, 13, ".")
        L(false, Ident, 14, "qux")
        L(false, LBracket, 17, "[")
        L(false, LBracket, 18, "[")
        L(false, Ident, 19, "quy")
        L(false, RBracket, 22, "]")
        L(false, RBracket, 23, "]")
        L(false, LBracket, 24, "[")
        L(false, Ident, 25, "quz")
        L(false, RBracket, 28, "]")
        L(false, RBrace, 29, "}")
        "###);
    }

    /// Numbers can be represented in decimal or hexadecimal (prefixed with 0x).
    #[test]
    fn test_numeric_literals() {
        assert_snapshot!(lexemes(r#"{123 0x123 def 0xdef}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, NumDec, 1, "123")
        L(true, NumHex, 7, "123")
        L(true, Ident, 11, "def")
        L(true, NumHex, 17, "def")
        L(false, RBrace, 20, "}")
        "###);
    }

    /// Numbers can optionally be grouped using underscores. Underscores cannot be trailing, but
    /// otherwise can appear in every position
    #[test]
    fn test_numeric_literal_underscores() {
        assert_snapshot!(lexemes(r#"{123_456 0x12_ab_de _123}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, NumDec, 1, "123_456")
        L(true, NumHex, 11, "12_ab_de")
        L(true, Unexpected, 20, "_")
        L(false, NumDec, 21, "123")
        L(false, RBrace, 24, "}")
        "###);
    }

    /// Address literals are numbers prefixed with '@' -- typically, they are hexadecimal numbers
    /// but both kinds are supported.
    #[test]
    fn test_address_literals() {
        assert_snapshot!(lexemes(r#"{@123 @0x123}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, At, 1, "@")
        L(false, NumDec, 2, "123")
        L(true, At, 6, "@")
        L(false, NumHex, 9, "123")
        L(false, RBrace, 12, "}")
        "###);
    }

    /// If the hexadecimal token is incomplete, it is not recognised as a number.
    #[test]
    fn test_incomplete_hexadecimal() {
        assert_snapshot!(lexemes(r#"{0x}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, NumDec, 1, "0")
        L(false, Ident, 2, "x")
        L(false, RBrace, 3, "}")
        "###);
    }

    /// Vector literals are always prefixed by the 'vector' keyword. Empty vectors must specify a
    /// type parameter (which is optional for non-empty vectors).
    #[test]
    fn test_vector_literals() {
        assert_snapshot!(lexemes(r#"{vector[1, 2, 3] vector<u32> vector[4u64]}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, Ident, 1, "vector")
        L(false, LBracket, 7, "[")
        L(false, NumDec, 8, "1")
        L(false, Comma, 9, ",")
        L(true, NumDec, 11, "2")
        L(false, Comma, 12, ",")
        L(true, NumDec, 14, "3")
        L(false, RBracket, 15, "]")
        L(true, Ident, 17, "vector")
        L(false, LAngle, 23, "<")
        L(false, Ident, 24, "u32")
        L(false, RAngle, 27, ">")
        L(true, Ident, 29, "vector")
        L(false, LBracket, 35, "[")
        L(false, NumDec, 36, "4")
        L(false, Ident, 37, "u64")
        L(false, RBracket, 40, "]")
        L(false, RBrace, 41, "}")
        "###);
    }

    /// Struct types are fully-qualified, with a numerical (hexadecimal) address.
    #[test]
    fn test_types() {
        assert_snapshot!(lexemes(r#"{0x2::table::Table<address, 0x2::coin::Coin<0x2::sui::SUI>>}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, NumHex, 3, "2")
        L(false, CColon, 4, "::")
        L(false, Ident, 6, "table")
        L(false, CColon, 11, "::")
        L(false, Ident, 13, "Table")
        L(false, LAngle, 18, "<")
        L(false, Ident, 19, "address")
        L(false, Comma, 26, ",")
        L(true, NumHex, 30, "2")
        L(false, CColon, 31, "::")
        L(false, Ident, 33, "coin")
        L(false, CColon, 37, "::")
        L(false, Ident, 39, "Coin")
        L(false, LAngle, 43, "<")
        L(false, NumHex, 46, "2")
        L(false, CColon, 47, "::")
        L(false, Ident, 49, "sui")
        L(false, CColon, 52, "::")
        L(false, Ident, 54, "SUI")
        L(false, RAngle, 57, ">")
        L(false, RAngle, 58, ">")
        L(false, RBrace, 59, "}")
        "###);
    }

    /// A positional struct literal is a struct type followed by its (positional) fields, separated
    /// by commas, surrounded by parentheses.
    #[test]
    fn test_positional_struct_literals() {
        assert_snapshot!(lexemes(r#"{0x2::balance::Balance<0x2::sui::SUI>(42u64)}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, NumHex, 3, "2")
        L(false, CColon, 4, "::")
        L(false, Ident, 6, "balance")
        L(false, CColon, 13, "::")
        L(false, Ident, 15, "Balance")
        L(false, LAngle, 22, "<")
        L(false, NumHex, 25, "2")
        L(false, CColon, 26, "::")
        L(false, Ident, 28, "sui")
        L(false, CColon, 31, "::")
        L(false, Ident, 33, "SUI")
        L(false, RAngle, 36, ">")
        L(false, LParen, 37, "(")
        L(false, NumDec, 38, "42")
        L(false, Ident, 40, "u64")
        L(false, RParen, 43, ")")
        L(false, RBrace, 44, "}")
        "###);
    }

    /// Struct literals can also include field names -- these are purely informational, they don't
    /// affect the encoded output.
    #[test]
    fn test_struct_literals() {
        assert_snapshot!(lexemes(r#"{0x2::coin::Coin<0x2::sui::SUI> { id: @0x123, value: 42u64 }}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, NumHex, 3, "2")
        L(false, CColon, 4, "::")
        L(false, Ident, 6, "coin")
        L(false, CColon, 10, "::")
        L(false, Ident, 12, "Coin")
        L(false, LAngle, 16, "<")
        L(false, NumHex, 19, "2")
        L(false, CColon, 20, "::")
        L(false, Ident, 22, "sui")
        L(false, CColon, 25, "::")
        L(false, Ident, 27, "SUI")
        L(false, RAngle, 30, ">")
        L(true, LBrace, 32, "{")
        L(true, Ident, 34, "id")
        L(false, Colon, 36, ":")
        L(true, At, 38, "@")
        L(false, NumHex, 41, "123")
        L(false, Comma, 44, ",")
        L(true, Ident, 46, "value")
        L(false, Colon, 51, ":")
        L(true, NumDec, 53, "42")
        L(false, Ident, 55, "u64")
        L(true, RBrace, 59, "}")
        L(false, RBrace, 60, "}")
        "###);
    }

    /// Enums are like structs but with an additional variant component. The variant must at least
    /// specify the variant index, and can optionally specify a variant name, which is only
    /// relevant for documentation purposes (it does not affect the encoding).
    #[test]
    fn test_enum_literals() {
        assert_snapshot!(lexemes(r#"{0x2::option::Option<u64>::1(42) 0x2::option::Option<u64>::Some#1(43)}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, NumHex, 3, "2")
        L(false, CColon, 4, "::")
        L(false, Ident, 6, "option")
        L(false, CColon, 12, "::")
        L(false, Ident, 14, "Option")
        L(false, LAngle, 20, "<")
        L(false, Ident, 21, "u64")
        L(false, RAngle, 24, ">")
        L(false, CColon, 25, "::")
        L(false, NumDec, 27, "1")
        L(false, LParen, 28, "(")
        L(false, NumDec, 29, "42")
        L(false, RParen, 31, ")")
        L(true, NumHex, 35, "2")
        L(false, CColon, 36, "::")
        L(false, Ident, 38, "option")
        L(false, CColon, 44, "::")
        L(false, Ident, 46, "Option")
        L(false, LAngle, 52, "<")
        L(false, Ident, 53, "u64")
        L(false, RAngle, 56, ">")
        L(false, CColon, 57, "::")
        L(false, Ident, 59, "Some")
        L(false, Pound, 63, "#")
        L(false, NumDec, 64, "1")
        L(false, LParen, 65, "(")
        L(false, NumDec, 66, "43")
        L(false, RParen, 68, ")")
        L(false, RBrace, 69, "}")
        "###);
    }

    /// Tokenizing three kinds of string literals hex, binary, and regular.
    #[test]
    fn string_literals() {
        assert_snapshot!(lexemes(r#"{x'0f00' b'bar' 'baz'}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, Ident, 1, "x")
        L(false, String, 3, "0f00")
        L(true, Ident, 9, "b")
        L(false, String, 11, "bar")
        L(true, String, 17, "baz")
        L(false, RBrace, 21, "}")
        "###);
    }

    /// Make sure the string does not stop early on an escaped quote, it's fine to escape random
    /// characters, and an escaped backslash does not eat the closing quote.
    #[test]
    fn test_string_literal_escapes() {
        assert_snapshot!(lexemes(r#"{'\' \x \\'}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, String, 2, "\\' \\x \\\\")
        L(false, RBrace, 11, "}")
        "###);
    }

    /// If the string literal is not closed, the whole sequence is treated as an "unexpected"
    /// token.
    #[test]
    fn test_string_literal_trailing() {
        assert_snapshot!(lexemes(r#"{'foo bar}"#), @r###"
        L(false, LBrace, 0, "{")
        L(false, Unexpected, 1, "'foo bar}")
        "###);
    }
}
