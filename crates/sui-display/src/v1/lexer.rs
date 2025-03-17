// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

/// Lexer for Display V1 format strings. Format strings are a mix of text and expressions.
/// Expressions are enclosed in curly braces and may contain identifiers separated by dots (a path
/// of field accesses).
#[derive(Debug)]
pub(crate) struct Lexer<'s> {
    /// Remaining input to be tokenized.
    src: &'s str,

    /// The number of bytes tokenized so far.
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
    /// '\X' where X is any byte.
    Escaped,
    /// A potential field identifier
    Ident,
    /// '{'
    LCurl,
    /// '}'
    RCurl,
    /// A strand of text.
    Text,
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
            b'\\' if bytes.len() > 1 => {
                self.take(T::Escaped, 1); // discard the backslash
                self.take(T::Escaped, 1)
            }
            b'\\' => self.take(T::Text, 1),
            b'{' => {
                self.mode = Mode::Expr;
                self.take(T::LCurl, 1)
            }
            // This is not a valid token within text, but recognise it so that the parser can
            // produce a better error message.
            b'}' => self.take(T::RCurl, 1),
            _ => self.take_until(T::Text, |c| ['\\', '{', '}'].contains(&c)),
        })
    }

    /// Assuming the lexer is in expression mode, return the next expression token.
    fn next_expr_token(&mut self) -> Option<Lexeme<'s>> {
        self.skip_whitespace();

        use Token as T;
        Some(match self.src.as_bytes().first()? {
            // { is not a valid token within an expression, but recognise it so that the parser can
            // produce a better error message.
            b'{' => self.take(T::LCurl, 1),
            b'}' => {
                self.mode = Mode::Text;
                self.take(T::RCurl, 1)
            }
            b'.' => self.take(T::Dot, 1),
            // The lexer takes a very liberal definition of "identifier", the parser will check
            // whether the identifier is actually valid.
            _ => self.take_until(T::Ident, |c| {
                c.is_whitespace() || c == '.' || c == '{' || c == '}'
            }),
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
            L(T::Escaped, _, s) => write!(f, "escaped character '\\{s}'"),
            L(T::LCurl, _, _) => write!(f, "'{{'"),
            L(T::RCurl, _, _) => write!(f, "'}}'"),
            L(T::Ident, _, s) => write!(f, "identifier {s:?}"),
            L(T::Dot, _, _) => write!(f, "'.'"),
        }?;

        write!(f, " at offset {}", self.1)
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Token as T;
        match self {
            T::Text => write!(f, "text"),
            T::Escaped => write!(f, "an escaped character"),
            T::LCurl => write!(f, "'{{'"),
            T::RCurl => write!(f, "'}}'"),
            T::Ident => write!(f, "an identifier"),
            T::Dot => write!(f, "'.'"),
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

    /// Simple test for a raw literal string.
    #[test]
    fn test_all_text() {
        let lexer = Lexer::new("foo bar");
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(lexemes, vec![L(T::Text, 0, "foo bar")]);
    }

    /// Escape sequences are all text, but
    #[test]
    fn test_escapes() {
        let lexer = Lexer::new(r#"foo \b\{ar\}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::Escaped, 5, "b"),
                L(T::Escaped, 7, "{"),
                L(T::Text, 8, "ar"),
                L(T::Escaped, 11, "}"),
            ]
        );
    }

    /// If the last character is a backslash, then treat it as just a backslash, not the start of
    /// an escape sequence.
    #[test]
    fn test_trailing_escape() {
        let lexer = Lexer::new(r#"foo bar\"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(lexemes, vec![L(T::Text, 0, "foo bar"), L(T::Text, 7, "\\")],);
    }

    /// Text inside curly braces is tokenized as if it's an expression.
    #[test]
    fn test_expressions() {
        let lexer = Lexer::new(r#"foo {bar}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LCurl, 4, "{"),
                L(T::Ident, 5, "bar"),
                L(T::RCurl, 8, "}"),
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
                L(T::LCurl, 4, "{"),
                L(T::Ident, 7, "bar"),
                L(T::RCurl, 13, "}"),
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
                L(T::LCurl, 4, "{"),
                L(T::Ident, 5, "bar"),
                L(T::Dot, 8, "."),
                L(T::Ident, 10, "baz"),
                L(T::Dot, 15, "."),
                L(T::Ident, 17, "qux"),
                L(T::RCurl, 20, "}"),
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
                L(T::LCurl, 4, "{"),
                L(T::Ident, 5, "bar"),
                L(T::Dot, 8, "."),
                L(T::Ident, 9, "baz"),
                L(T::RCurl, 12, "}"),
                L(T::Text, 13, " qux "),
                L(T::LCurl, 18, "{"),
                L(T::Ident, 19, "quy"),
                L(T::Dot, 22, "."),
                L(T::Ident, 23, "quz"),
                L(T::RCurl, 26, "}"),
            ],
        );
    }

    /// Left curlies are not valid inside expressions and right curlies are not valid inside text
    /// strands, but we still tokenize them so that we can detect them during parsing and return
    /// ane error message.
    #[test]
    fn test_misplaced_curlies() {
        let lexer = Lexer::new(r#"foo}{bar{}}"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo"),
                L(T::RCurl, 3, "}"),
                L(T::LCurl, 4, "{"),
                L(T::Ident, 5, "bar"),
                L(T::LCurl, 8, "{"),
                L(T::RCurl, 9, "}"),
                L(T::RCurl, 10, "}"),
            ],
        );
    }

    /// The lexer is very permissive about what it considers an identifier, this allows it to
    /// gather more context without failing, while the parser will check that the identifier is
    /// valid.
    #[test]
    fn test_strange_identifiers() {
        let lexer = Lexer::new(r#"{ not-really . an! . ident#fier? }"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LCurl, 0, "{"),
                L(T::Ident, 2, "not-really"),
                L(T::Dot, 13, "."),
                L(T::Ident, 15, "an!"),
                L(T::Dot, 19, "."),
                L(T::Ident, 21, "ident#fier?"),
                L(T::RCurl, 33, "}"),
            ],
        );
    }

    /// The lexer should correctly identify backslashes that signify escapes vs backslashes that
    /// are literal.
    #[test]
    fn test_escape_chain() {
        let lexer = Lexer::new(r#"\\\\\\\\\"#);
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Escaped, 1, r#"\"#),
                L(T::Escaped, 3, r#"\"#),
                L(T::Escaped, 5, r#"\"#),
                L(T::Escaped, 7, r#"\"#),
                L(T::Text, 8, r#"\"#),
            ],
        );
    }
}
