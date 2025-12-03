// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;

use crate::lexer::{Lexeme, Token};

/// Recursive descent parser for format strings. This struct is intended to be lightweight to
/// support efficient backtracking. Backtracking is implemented by operating on a copy of the
/// parser's state, and only updating the original when the copy has reached a known good position.
#[derive(Copy, Clone)]
pub(crate) struct Parser<'l, 's> {
    lexemes: &'l [Lexeme<'s>],
}

/// Structured representation of a format string, starting with a text prefix (the head), followed
/// by interleaved binders followed by text suffixes (the tail).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Format<'s> {
    pub head: Cow<'s, str>,
    pub tail: Vec<(Option<syn::Type>, Cow<'s, str>)>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Unexpected {actual} at offset {offset}, expected {expect}")]
    Unexpected {
        offset: usize,
        actual: Token,
        expect: Token,
    },

    #[error("Unexpected end of format string, expected: {expect:?}")]
    UnexpectedEos { expect: Token },

    #[error("Error parsing type for binder at offset {offset}: {source}")]
    TypeParse { offset: usize, source: syn::Error },
}

/// Recursive descent parser recognizing the following grammar:
///
///   format        ::= text? (bind text?)*
///   text          ::= part +
///   part          ::= lcurly_escape | rcurly_escape | TEXT
///   bind          ::= '{' TEXT? '}'
///   lcurly_escape ::= '{' '{'
///   rcurly_escape ::= '}' '}'
///
/// The parser reads tokens from the front, consuming them when it is able to successfully parse a
/// non-terminal. When parsing fails, the parser may be left in an indeterminate state.
/// Backtracking requires explicitly copying the parser state.
impl<'l, 's> Parser<'l, 's> {
    /// Constructs a new parser instance that will consume the given `lexemes`.
    pub(crate) fn new(lexemes: &'l [Lexeme<'s>]) -> Self {
        Self { lexemes }
    }

    /// Entrypoint to the parser. Consumes the entire remaining output (and therefore also the
    /// parser).
    pub(crate) fn format(mut self) -> Result<Format<'s>, Error> {
        let head = self.text_opt();

        let mut tail = vec![];
        while let Some(bind) = self.bind()? {
            let suffix = self.text_opt();
            tail.push((bind, suffix));
        }

        Ok(Format { head, tail })
    }

    /// Parse a strand of text. The parser is left in its initial state and an empty string is
    /// returned if there was no text to parse.
    fn text_opt(&mut self) -> Cow<'s, str> {
        let mut copy = *self;
        copy.text().map_or(Cow::Borrowed(""), |t| {
            *self = copy;
            t
        })
    }

    /// Parse a strand of text by gathering together as many `part`s as possible. Errors if there
    /// is not at least one `part` to consume.
    fn text(&mut self) -> Result<Cow<'s, str>, Error> {
        let mut text = self.part()?;

        let mut copy = *self;
        while let Ok(part) = copy.part() {
            text += part;
            *self = copy;
        }

        Ok(text)
    }

    /// Parses any of the three possible `part`s of a text strand: a string of text, or an escaped
    /// curly brace. Errors if the token stream starts with a binder.
    fn part(&mut self) -> Result<Cow<'s, str>, Error> {
        let mut copy = *self;
        if let Ok(s) = copy.lcurly_escape() {
            *self = copy;
            return Ok(s);
        }

        let mut copy = *self;
        if let Ok(s) = copy.rcurly_escape() {
            *self = copy;
            return Ok(s);
        }

        let Lexeme(_, _, text) = self.eat(Token::Text)?;
        Ok(Cow::Borrowed(text))
    }

    /// Parses as an escaped left curly brace (two curly brace tokens).
    fn lcurly_escape(&mut self) -> Result<Cow<'s, str>, Error> {
        use Token as T;
        self.eat(T::LCurl)?;
        self.eat(T::LCurl)?;
        Ok(Cow::Borrowed("{"))
    }

    /// Parses as an escaped right curly brace (two curly brace tokens).
    fn rcurly_escape(&mut self) -> Result<Cow<'s, str>, Error> {
        use Token as T;
        self.eat(T::RCurl)?;
        self.eat(T::RCurl)?;
        Ok(Cow::Borrowed("}"))
    }

    /// Parses a binding (curly braces optionally containing a type).
    ///
    /// Returns `Ok(None)` if there are no tokens left, `Ok(Some(None))` for a binding with no
    /// type, `Ok(Some(Some(type)))` for a binding with a type, or an error if the binding is
    /// malformed.
    fn bind(&mut self) -> Result<Option<Option<syn::Type>>, Error> {
        if self.lexemes.is_empty() {
            return Ok(None);
        }

        self.eat(Token::LCurl)?;
        if self.peek(Token::RCurl).is_ok() {
            self.eat(Token::RCurl)?;
            return Ok(Some(None));
        }

        let Lexeme(_, offset, type_) = self.eat(Token::Text)?;
        self.eat(Token::RCurl)?;

        let type_ = syn::parse_str(type_).map_err(|source| Error::TypeParse { offset, source })?;
        Ok(Some(Some(type_)))
    }

    /// Consume the next token (returning it) as long as it matches `expect`.
    fn eat(&mut self, expect: Token) -> Result<Lexeme<'s>, Error> {
        match self.peek(expect) {
            Ok(l) => {
                self.lexemes = &self.lexemes[1..];
                Ok(l)
            }
            Err(Some(l)) => Err(Error::Unexpected {
                offset: l.1,
                actual: l.0,
                expect,
            }),
            Err(None) => Err(Error::UnexpectedEos { expect }),
        }
    }

    /// Look at the next token and check that it matches `expect` without consuming it. Returns
    /// the Ok of the lexeme if its token matches, otherwise it returns an error that either
    /// contains the lexeme that did not match or None if there are no tokens left.
    fn peek(&self, expect: Token) -> Result<Lexeme<'s>, Option<Lexeme<'s>>> {
        match self.lexemes.first() {
            Some(l) if l.0 == expect => Ok(*l),
            Some(l) => Err(Some(*l)),
            None => Err(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Lexer;

    use super::*;

    /// Test helper for lexing and parsing a format string.
    fn parse(s: &str) -> Result<Format<'_>, Error> {
        let lexemes: Vec<_> = Lexer::new(s).collect();
        Parser::new(&lexemes).format()
    }

    /// Parse a Rust type from a string, to compare against binds in tests.
    fn type_(s: &str) -> Option<syn::Type> {
        Some(syn::parse_str(s).unwrap())
    }

    #[test]
    fn test_no_binds() {
        // A simple format string with no binders will gather everything into the head.
        assert_eq!(
            parse("foo").unwrap(),
            Format {
                head: "foo".into(),
                tail: vec![]
            }
        );
    }

    #[test]
    fn test_single_bind() {
        // A binder is parsed without its surrounding binders as a type, and splits the format
        // string in two.
        assert_eq!(
            parse("foo = {Text} AND bar = 42").unwrap(),
            Format {
                head: "foo = ".into(),
                tail: vec![(type_("Text"), " AND bar = 42".into())]
            },
        );
    }

    #[test]
    fn test_multiple_binds() {
        // When there are multiple binders the parser needs to detect the gap between binders.
        assert_eq!(
            parse("foo = {Text} AND (bar < {BigInt} OR bar > 5)").unwrap(),
            Format {
                head: "foo = ".into(),
                tail: vec![
                    (type_("Text"), " AND (bar < ".into()),
                    (type_("BigInt"), " OR bar > 5)".into()),
                ],
            },
        );
    }

    #[test]
    fn test_ends_with_a_bind() {
        // If the format string ends with a binder, the parser still needs to find an empty suffix
        // binder.
        assert_eq!(
            parse("bar BETWEEN {BigInt} AND {BigInt}").unwrap(),
            Format {
                head: "bar BETWEEN ".into(),
                tail: vec![
                    (type_("BigInt"), " AND ".into()),
                    (type_("BigInt"), "".into()),
                ],
            },
        );
    }

    #[test]
    fn test_escaped_curlies() {
        // Escaped curlies are de-duplicated in the parsed output, but the parser does not break up
        // strands of format string on them.
        assert_eq!(
            parse("foo LIKE '{{bar%'").unwrap(),
            Format {
                head: "foo LIKE '{bar%'".into(),
                tail: vec![],
            },
        );
    }

    #[test]
    fn test_curly_nest() {
        // This input can be tricky to parse if the lexer treats escaped curlies as a single token.
        assert_eq!(
            parse("{{{Bool}}}").unwrap(),
            Format {
                head: "{".into(),
                tail: vec![(type_("Bool"), "}".into())],
            },
        );
    }

    #[test]
    fn test_bind_unexpected_token() {
        // Error if the binder is not properly closed.
        assert!(matches!(
            parse("{Bool{").unwrap_err(),
            Error::Unexpected {
                offset: 5,
                actual: Token::LCurl,
                expect: Token::RCurl
            },
        ));
    }

    #[test]
    fn test_bind_no_type() {
        // Empty binders are supported.
        assert_eq!(
            parse("foo = {}").unwrap(),
            Format {
                head: "foo = ".into(),
                tail: vec![(None, "".into())],
            }
        );
    }

    #[test]
    fn test_bind_unfinished() {
        // Error if the binder does not contain a type.
        assert!(matches!(
            parse("foo = {").unwrap_err(),
            Error::UnexpectedEos {
                expect: Token::Text,
            },
        ));
    }

    #[test]
    fn test_bind_no_rcurly() {
        // Error if the binder ends before it is closed.
        assert!(matches!(
            parse("foo = {Text").unwrap_err(),
            Error::UnexpectedEos {
                expect: Token::RCurl,
            },
        ));
    }

    #[test]
    fn test_bind_bad_type() {
        // Failure to parse the binder as a Rust type is also an error for this parser.
        assert!(matches!(
            parse("foo = {not a type}").unwrap_err(),
            Error::TypeParse { offset: 7, .. },
        ));
    }
}
