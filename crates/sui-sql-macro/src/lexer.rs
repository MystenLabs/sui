// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

/// Lexer for SQL format strings. Format string can contain regular text, or binders surrounded by
/// curly braces. Curly braces are escaped by doubling them up.
pub(crate) struct Lexer<'s> {
    src: &'s str,
    off: usize,
}

/// A lexeme is a token along with its offset in the source string, and the slice of source string
/// that it originated from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lexeme<'s>(pub Token, pub usize, pub &'s str);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Token {
    /// '{'
    LCurl,
    /// '}'
    RCurl,
    /// Any other text
    Text,
}

impl<'s> Lexer<'s> {
    pub(crate) fn new(src: &'s str) -> Self {
        Self { src, off: 0 }
    }
}

impl<'s> Iterator for Lexer<'s> {
    type Item = Lexeme<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        let off = self.off;
        let bytes = self.src.as_bytes();
        let fst = bytes.first()?;

        Some(match fst {
            b'{' => {
                let span = &self.src[..1];
                self.src = &self.src[1..];
                self.off += 1;
                Lexeme(Token::LCurl, off, span)
            }

            b'}' => {
                let span = &self.src[..1];
                self.src = &self.src[1..];
                self.off += 1;
                Lexeme(Token::RCurl, off, span)
            }

            _ => {
                let end = self.src.find(['{', '}']).unwrap_or(self.src.len());
                let span = &self.src[..end];
                self.src = &self.src[end..];
                self.off += end;
                Lexeme(Token::Text, off, span)
            }
        })
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Token as T;
        match self {
            T::LCurl => write!(f, "'{{'"),
            T::RCurl => write!(f, "'}}'"),
            T::Text => write!(f, "text"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Lexeme as L;
    use Token as T;

    /// Lexing source material that only contains text and no curly braces.
    #[test]
    fn test_all_text() {
        let lexer = Lexer::new("foo bar");
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(lexemes, vec![L(T::Text, 0, "foo bar")]);
    }

    /// When the lexer encounters curly braces in the source material it breaks up the text with
    /// curly brace tokens.
    #[test]
    fn test_curlies() {
        let lexer = Lexer::new("foo {bar} baz");
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LCurl, 4, "{"),
                L(T::Text, 5, "bar"),
                L(T::RCurl, 8, "}"),
                L(T::Text, 9, " baz"),
            ],
        );
    }

    /// Repeated curly braces next to each other are used to escape those braces.
    #[test]
    fn test_escape_curlies() {
        let lexer = Lexer::new("foo {{bar}} baz");
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::Text, 0, "foo "),
                L(T::LCurl, 4, "{"),
                L(T::LCurl, 5, "{"),
                L(T::Text, 6, "bar"),
                L(T::RCurl, 9, "}"),
                L(T::RCurl, 10, "}"),
                L(T::Text, 11, " baz"),
            ],
        );
    }

    /// Each curly brace is given its own token so that the parser can parse this as an escaped
    /// opening curly followed by an empty binder, followed by a literal closing curly. If the
    /// lexer was responsible for detecting escaped curlies, it would eagerly detect the escaped
    /// closing curly and then the closing curly for the binder.
    #[test]
    fn test_combination_curlies() {
        let lexer = Lexer::new("{{{}}}");
        let lexemes: Vec<_> = lexer.collect();
        assert_eq!(
            lexemes,
            vec![
                L(T::LCurl, 0, "{"),
                L(T::LCurl, 1, "{"),
                L(T::LCurl, 2, "{"),
                L(T::RCurl, 3, "}"),
                L(T::RCurl, 4, "}"),
                L(T::RCurl, 5, "}"),
            ],
        );
    }
}
