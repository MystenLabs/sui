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

/// A lexeme is a token along with its offset in the source string, and the slice of source string
/// that it originated from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lexeme<'s>(pub Token, pub usize, pub &'s str);

/// Like [Lexeme] but owns the slice of source string. Useful for capturing context in an error
/// message.
#[derive(Debug)]
pub(crate) struct OwnedLexeme(pub Token, pub usize, pub String);

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

    /// Whitespace around expressions.
    Whitespace,
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
                self.take(T::LLBrace, 1)
            }

            b'{' => {
                self.level += 1;
                self.take(T::LBrace, 1)
            }

            b'}' if bytes.get(1) == Some(&b'}') => {
                self.advance(1);
                self.take(T::RRBrace, 1)
            }

            // This is not a valid token within text, but is recognised so that the parser can
            // produce a better error message. `level` is not decremenetd because we should already
            // been in text mode, meaning the level is already 0, and a decrement would underflow
            // it.
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

            b'{' => {
                self.level += 1;
                self.take(T::LBrace, 1)
            }

            b'[' => self.take(T::LBracket, 1),

            b'(' => self.take(T::LParen, 1),

            b'|' => self.take(T::Pipe, 1),

            b'#' => self.take(T::Pound, 1),

            b'>' => self.take(T::RAngle, 1),

            b'}' => {
                self.level -= 1;
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
            L(T::Unexpected, _, s) => write!(f, "{s:?}"),
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
            T::Unexpected => write!(f, "unexpected input"),
            T::Whitespace => write!(f, "whitespace"),
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
            .map(|L(t, o, s)| format!("L({t:?}, {o:?}, {s:?})"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Simple test for a  raw literal string.
    #[test]
    fn test_all_text() {
        assert_snapshot!(lexemes("foo bar"), @r###"L(Text, 0, "foo bar")"###);
    }

    /// Escape sequences are all text, but they will be split into multiple tokens.
    #[test]
    fn test_escapes() {
        assert_snapshot!(lexemes(r#"foo {{bar}}"#), @r###"
        L(Text, 0, "foo ")
        L(LLBrace, 5, "{")
        L(Text, 6, "bar")
        L(RRBrace, 10, "}")
        "###);
    }

    /// Text inside braces is tokenized as if it's an expression.
    #[test]
    fn test_expressions() {
        assert_snapshot!(lexemes(r#"foo {bar}"#), @r###"
        L(Text, 0, "foo ")
        L(LBrace, 4, "{")
        L(Ident, 5, "bar")
        L(RBrace, 8, "}")
        "###);
    }

    /// Expressions are tokenized to ignore whitespace.
    #[test]
    fn test_expression_whitespace() {
        assert_snapshot!(lexemes(r#"foo {  bar   }"#), @r###"
        L(Text, 0, "foo ")
        L(LBrace, 4, "{")
        L(Whitespace, 5, "  ")
        L(Ident, 7, "bar")
        L(Whitespace, 10, "   ")
        L(RBrace, 13, "}")
        "###);
    }

    /// Field names are separated by dots in an expression.
    #[test]
    fn test_expression_dots() {
        assert_snapshot!(lexemes(r#"foo {bar. baz  . qux}"#), @r###"
        L(Text, 0, "foo ")
        L(LBrace, 4, "{")
        L(Ident, 5, "bar")
        L(Dot, 8, ".")
        L(Whitespace, 9, " ")
        L(Ident, 10, "baz")
        L(Whitespace, 13, "  ")
        L(Dot, 15, ".")
        L(Whitespace, 16, " ")
        L(Ident, 17, "qux")
        L(RBrace, 20, "}")
        "###);
    }

    /// Multiple expressions test switching and back and forth between lexer modes.
    #[test]
    fn test_multiple_expressions() {
        assert_snapshot!(lexemes(r#"foo {bar.baz} qux {quy.quz}"#), @r###"
        L(Text, 0, "foo ")
        L(LBrace, 4, "{")
        L(Ident, 5, "bar")
        L(Dot, 8, ".")
        L(Ident, 9, "baz")
        L(RBrace, 12, "}")
        L(Text, 13, " qux ")
        L(LBrace, 18, "{")
        L(Ident, 19, "quy")
        L(Dot, 22, ".")
        L(Ident, 23, "quz")
        L(RBrace, 26, "}")
        "###);
    }

    /// Expressions can include nested curly braces. Meeting the first well-bracketed closing curly
    /// brace should not cause the lexer to exit expression mode.
    #[test]
    fn test_nested_curlies() {
        assert_snapshot!(lexemes(r#"foo {bar {baz} qux}"#), @r###"
        L(Text, 0, "foo ")
        L(LBrace, 4, "{")
        L(Ident, 5, "bar")
        L(Whitespace, 8, " ")
        L(LBrace, 9, "{")
        L(Ident, 10, "baz")
        L(RBrace, 13, "}")
        L(Whitespace, 14, " ")
        L(Ident, 15, "qux")
        L(RBrace, 18, "}")
        "###);
    }

    /// The lexer will still tokenize curlies even if they are not balanced.
    #[test]
    fn test_unbalanced_curlies() {
        assert_snapshot!(lexemes(r#"foo}{bar{}}"#), @r###"
        L(Text, 0, "foo")
        L(RBrace, 3, "}")
        L(LBrace, 4, "{")
        L(Ident, 5, "bar")
        L(LBrace, 8, "{")
        L(RBrace, 9, "}")
        L(RBrace, 10, "}")
        "###);
    }

    /// Unexpected characters are tokenized so that the parser can produce an error.
    #[test]
    fn test_unexpected_characters() {
        assert_snapshot!(lexemes(r#"anything goes {? % ! 🔥}"#), @r###"
        L(Text, 0, "anything goes ")
        L(LBrace, 14, "{")
        L(Unexpected, 15, "?")
        L(Whitespace, 16, " ")
        L(Unexpected, 17, "%")
        L(Whitespace, 18, " ")
        L(Unexpected, 19, "!")
        L(Whitespace, 20, " ")
        L(Unexpected, 21, "🔥")
        L(RBrace, 25, "}")
        "###);
    }

    // Escaped curlies shouldn't be tokenized greedily. '{{{' in text mode should be tokenized as
    // '{{' and '{', while '}}}' in expr mode should be tokenized as '}' and '}}'. This test
    // exercises these and similar cases.
    #[test]
    fn test_triple_curlies() {
        assert_snapshot!(lexemes(r#"foo {{{bar} {baz}}} }}} { {{ } qux"#), @r###"
        L(Text, 0, "foo ")
        L(LLBrace, 5, "{")
        L(LBrace, 6, "{")
        L(Ident, 7, "bar")
        L(RBrace, 10, "}")
        L(Text, 11, " ")
        L(LBrace, 12, "{")
        L(Ident, 13, "baz")
        L(RBrace, 16, "}")
        L(RRBrace, 18, "}")
        L(Text, 19, " ")
        L(RRBrace, 21, "}")
        L(RBrace, 22, "}")
        L(Text, 23, " ")
        L(LBrace, 24, "{")
        L(Whitespace, 25, " ")
        L(LBrace, 26, "{")
        L(LBrace, 27, "{")
        L(Whitespace, 28, " ")
        L(RBrace, 29, "}")
        L(Whitespace, 30, " ")
        L(Ident, 31, "qux")
        "###);
    }

    /// Pipes separate top-level expressions, but are only parsed inside expressions, not inside
    /// text.
    #[test]
    fn test_alternates() {
        assert_snapshot!(lexemes(r#"foo | {bar | baz.qux} | quy"#), @r###"
        L(Text, 0, "foo | ")
        L(LBrace, 6, "{")
        L(Ident, 7, "bar")
        L(Whitespace, 10, " ")
        L(Pipe, 11, "|")
        L(Whitespace, 12, " ")
        L(Ident, 13, "baz")
        L(Dot, 16, ".")
        L(Ident, 17, "qux")
        L(RBrace, 20, "}")
        L(Text, 21, " | quy")
        "###);
    }

    // Display supports two kinds of index -- `foo[i]` and `bar[[j]]`. Unlike braces, doubly nested
    // brackets do not have their own token. The two cases are distinguished by the parser, which
    // uses significant whitespace to distinguish between two separate `]`'s vs a single `]]`.
    #[test]
    fn test_indices() {
        assert_snapshot!(lexemes(r#"foo {bar[baz].qux[[quy]][quz]}"#), @r###"
        L(Text, 0, "foo ")
        L(LBrace, 4, "{")
        L(Ident, 5, "bar")
        L(LBracket, 8, "[")
        L(Ident, 9, "baz")
        L(RBracket, 12, "]")
        L(Dot, 13, ".")
        L(Ident, 14, "qux")
        L(LBracket, 17, "[")
        L(LBracket, 18, "[")
        L(Ident, 19, "quy")
        L(RBracket, 22, "]")
        L(RBracket, 23, "]")
        L(LBracket, 24, "[")
        L(Ident, 25, "quz")
        L(RBracket, 28, "]")
        L(RBrace, 29, "}")
        "###);
    }

    /// Numbers can be represented in decimal or hexadecimal (prefixed with 0x).
    #[test]
    fn test_numeric_literals() {
        assert_snapshot!(lexemes(r#"{123 0x123 def 0xdef}"#), @r###"
        L(LBrace, 0, "{")
        L(NumDec, 1, "123")
        L(Whitespace, 4, " ")
        L(NumHex, 7, "123")
        L(Whitespace, 10, " ")
        L(Ident, 11, "def")
        L(Whitespace, 14, " ")
        L(NumHex, 17, "def")
        L(RBrace, 20, "}")
        "###);
    }

    /// Numbers can optionally be grouped using underscores. Underscores cannot be trailing, but
    /// otherwise can appear in every position
    #[test]
    fn test_numeric_literal_underscores() {
        assert_snapshot!(lexemes(r#"{123_456 0x12_ab_de _123}"#), @r###"
        L(LBrace, 0, "{")
        L(NumDec, 1, "123_456")
        L(Whitespace, 8, " ")
        L(NumHex, 11, "12_ab_de")
        L(Whitespace, 19, " ")
        L(Unexpected, 20, "_")
        L(NumDec, 21, "123")
        L(RBrace, 24, "}")
        "###);
    }

    /// Address literals are numbers prefixed with '@' -- typically, they are hexadecimal numbers
    /// but both kinds are supported.
    #[test]
    fn test_address_literals() {
        assert_snapshot!(lexemes(r#"{@123 @0x123}"#), @r###"
        L(LBrace, 0, "{")
        L(At, 1, "@")
        L(NumDec, 2, "123")
        L(Whitespace, 5, " ")
        L(At, 6, "@")
        L(NumHex, 9, "123")
        L(RBrace, 12, "}")
        "###);
    }

    /// If the hexadecimal token is incomplete, it is not recognised as a number.
    #[test]
    fn test_incomplete_hexadecimal() {
        assert_snapshot!(lexemes(r#"{0x}"#), @r###"
        L(LBrace, 0, "{")
        L(NumDec, 1, "0")
        L(Ident, 2, "x")
        L(RBrace, 3, "}")
        "###);
    }

    /// Vector literals are always prefixed by the 'vector' keyword. Empty vectors must specify a
    /// type parameter (which is optional for non-empty vectors).
    #[test]
    fn test_vector_literals() {
        assert_snapshot!(lexemes(r#"{vector[1, 2, 3] vector<u32> vector[4u64]}"#), @r###"
        L(LBrace, 0, "{")
        L(Ident, 1, "vector")
        L(LBracket, 7, "[")
        L(NumDec, 8, "1")
        L(Comma, 9, ",")
        L(Whitespace, 10, " ")
        L(NumDec, 11, "2")
        L(Comma, 12, ",")
        L(Whitespace, 13, " ")
        L(NumDec, 14, "3")
        L(RBracket, 15, "]")
        L(Whitespace, 16, " ")
        L(Ident, 17, "vector")
        L(LAngle, 23, "<")
        L(Ident, 24, "u32")
        L(RAngle, 27, ">")
        L(Whitespace, 28, " ")
        L(Ident, 29, "vector")
        L(LBracket, 35, "[")
        L(NumDec, 36, "4")
        L(Ident, 37, "u64")
        L(RBracket, 40, "]")
        L(RBrace, 41, "}")
        "###);
    }

    /// Struct types are fully-qualified, with a numerical (hexadecimal) address.
    #[test]
    fn test_types() {
        assert_snapshot!(lexemes(r#"{0x2::table::Table<address, 0x2::coin::Coin<0x2::sui::SUI>>}"#), @r###"
        L(LBrace, 0, "{")
        L(NumHex, 3, "2")
        L(CColon, 4, "::")
        L(Ident, 6, "table")
        L(CColon, 11, "::")
        L(Ident, 13, "Table")
        L(LAngle, 18, "<")
        L(Ident, 19, "address")
        L(Comma, 26, ",")
        L(Whitespace, 27, " ")
        L(NumHex, 30, "2")
        L(CColon, 31, "::")
        L(Ident, 33, "coin")
        L(CColon, 37, "::")
        L(Ident, 39, "Coin")
        L(LAngle, 43, "<")
        L(NumHex, 46, "2")
        L(CColon, 47, "::")
        L(Ident, 49, "sui")
        L(CColon, 52, "::")
        L(Ident, 54, "SUI")
        L(RAngle, 57, ">")
        L(RAngle, 58, ">")
        L(RBrace, 59, "}")
        "###);
    }

    /// A positional struct literal is a struct type followed by its (positional) fields, separated
    /// by commas, surrounded by parentheses.
    #[test]
    fn test_positional_struct_literals() {
        assert_snapshot!(lexemes(r#"{0x2::balance::Balance<0x2::sui::SUI>(42u64)}"#), @r###"
        L(LBrace, 0, "{")
        L(NumHex, 3, "2")
        L(CColon, 4, "::")
        L(Ident, 6, "balance")
        L(CColon, 13, "::")
        L(Ident, 15, "Balance")
        L(LAngle, 22, "<")
        L(NumHex, 25, "2")
        L(CColon, 26, "::")
        L(Ident, 28, "sui")
        L(CColon, 31, "::")
        L(Ident, 33, "SUI")
        L(RAngle, 36, ">")
        L(LParen, 37, "(")
        L(NumDec, 38, "42")
        L(Ident, 40, "u64")
        L(RParen, 43, ")")
        L(RBrace, 44, "}")
        "###);
    }

    /// Struct literals can also include field names -- these are purely informational, they don't
    /// affect the encoded output.
    #[test]
    fn test_struct_literals() {
        assert_snapshot!(lexemes(r#"{0x2::coin::Coin<0x2::sui::SUI> { id: @0x123, value: 42u64 }}"#), @r###"
        L(LBrace, 0, "{")
        L(NumHex, 3, "2")
        L(CColon, 4, "::")
        L(Ident, 6, "coin")
        L(CColon, 10, "::")
        L(Ident, 12, "Coin")
        L(LAngle, 16, "<")
        L(NumHex, 19, "2")
        L(CColon, 20, "::")
        L(Ident, 22, "sui")
        L(CColon, 25, "::")
        L(Ident, 27, "SUI")
        L(RAngle, 30, ">")
        L(Whitespace, 31, " ")
        L(LBrace, 32, "{")
        L(Whitespace, 33, " ")
        L(Ident, 34, "id")
        L(Colon, 36, ":")
        L(Whitespace, 37, " ")
        L(At, 38, "@")
        L(NumHex, 41, "123")
        L(Comma, 44, ",")
        L(Whitespace, 45, " ")
        L(Ident, 46, "value")
        L(Colon, 51, ":")
        L(Whitespace, 52, " ")
        L(NumDec, 53, "42")
        L(Ident, 55, "u64")
        L(Whitespace, 58, " ")
        L(RBrace, 59, "}")
        L(RBrace, 60, "}")
        "###);
    }

    /// Enums are like structs but with an additional variant component. The variant must at least
    /// specify the variant index, and can optionally specify a variant name, which is only
    /// relevant for documentation purposes (it does not affect the encoding).
    #[test]
    fn test_enum_literals() {
        assert_snapshot!(lexemes(r#"{0x2::option::Option<u64>::1(42) 0x2::option::Option<u64>::Some#1(43)}"#), @r###"
        L(LBrace, 0, "{")
        L(NumHex, 3, "2")
        L(CColon, 4, "::")
        L(Ident, 6, "option")
        L(CColon, 12, "::")
        L(Ident, 14, "Option")
        L(LAngle, 20, "<")
        L(Ident, 21, "u64")
        L(RAngle, 24, ">")
        L(CColon, 25, "::")
        L(NumDec, 27, "1")
        L(LParen, 28, "(")
        L(NumDec, 29, "42")
        L(RParen, 31, ")")
        L(Whitespace, 32, " ")
        L(NumHex, 35, "2")
        L(CColon, 36, "::")
        L(Ident, 38, "option")
        L(CColon, 44, "::")
        L(Ident, 46, "Option")
        L(LAngle, 52, "<")
        L(Ident, 53, "u64")
        L(RAngle, 56, ">")
        L(CColon, 57, "::")
        L(Ident, 59, "Some")
        L(Pound, 63, "#")
        L(NumDec, 64, "1")
        L(LParen, 65, "(")
        L(NumDec, 66, "43")
        L(RParen, 68, ")")
        L(RBrace, 69, "}")
        "###);
    }

    /// Tokenizing three kinds of string literals hex, binary, and regular.
    #[test]
    fn string_literals() {
        assert_snapshot!(lexemes(r#"{x'0f00' b'bar' 'baz'}"#), @r###"
        L(LBrace, 0, "{")
        L(Ident, 1, "x")
        L(String, 3, "0f00")
        L(Whitespace, 8, " ")
        L(Ident, 9, "b")
        L(String, 11, "bar")
        L(Whitespace, 15, " ")
        L(String, 17, "baz")
        L(RBrace, 21, "}")
        "###);
    }

    /// Make sure the string does not stop early on an escaped quote, it's fine to escape random
    /// characters, and an escaped backslash does not eat the closing quote.
    #[test]
    fn test_string_literal_escapes() {
        assert_snapshot!(lexemes(r#"{'\' \x \\'}"#), @r###"
        L(LBrace, 0, "{")
        L(String, 2, "\\' \\x \\\\")
        L(RBrace, 11, "}")
        "###);
    }

    /// If the string literal is not closed, the whole sequence is treated as an "unexpected"
    /// token.
    #[test]
    fn test_string_literal_trailing() {
        assert_snapshot!(lexemes(r#"{'foo bar}"#), @r###"
        L(LBrace, 0, "{")
        L(Unexpected, 1, "'foo bar}")
        "###);
    }
}
