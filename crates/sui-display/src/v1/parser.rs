// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::fmt;
use std::iter::Peekable;

use move_core_types::annotated_extractor::Element;
use move_core_types::identifier;

use crate::v1::lexer::Lexeme as L;
use crate::v1::lexer::Lexer;
use crate::v1::lexer::OwnedLexeme;
use crate::v1::lexer::Token as T;
use crate::v1::lexer::TokenSet;

/// A strand is a single component of a format string, it can either be a piece of literal text
/// that needs to be preserved in the output, or a reference to a nested field (as a sequence of
/// field accesses) in the object being displayed which will need to be fetched and interpolated.
#[derive(Debug, Eq, PartialEq)]
pub enum Strand<'s> {
    Text(Cow<'s, str>),
    Expr(Vec<Element<'s>>),
}

pub(crate) struct Parser<'s> {
    max_depth: usize,
    lexer: Peekable<Lexer<'s>>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid identifier {ident:?} at offset {off}")]
    InvalidIdentifier { ident: String, off: usize },

    #[error("Field access at offset {off} deeper than the maximum of {max}")]
    TooDeep { max: usize, off: usize },

    #[error("Unexpected end-of-string, expected {expect}")]
    UnexpectedEos { expect: TokenSet<'static> },

    #[error("Unexpected {actual}, expected {expect}")]
    UnexpectedToken {
        actual: OwnedLexeme,
        expect: TokenSet<'static>,
    },
}

/// Pattern match on the next token in the lexer, without consuming it. Returns an error if there
/// is no next token, or if the next token doesn't match any of the provided patterns. The error
/// enumerates all the tokens that were expected.
macro_rules! match_token {
    ($lexer:expr; $(L($($pat:path)|+, $off:pat, $slice:pat) => $expr:expr),+ $(,)?) => {{
        const EXPECTED: TokenSet = TokenSet(&[$($($pat),+),+]);

        match $lexer.peek().ok_or_else(|| Error::UnexpectedEos { expect: EXPECTED })? {
            $(&L($($pat)|+, $off, $slice) => $expr,)+
            &actual => return Err(Error::UnexpectedToken {
                actual: actual.detach(),
                expect: EXPECTED,
            }),
        }
    }};
}

/// Recursive descent parser for Display V1 format strings, parsing the following grammar:
///
///   format ::= strand*
///   strand ::= text | expr
///   text   ::= part+
///   part   ::= TEXT | ESCAPED
///   expr   ::= '{' IDENT ('.' IDENT)* '}'
///
/// The grammar has a lookahead of one token, and requires no backtracking.
impl<'s> Parser<'s> {
    /// Construct a new parser, consuming input from the `src` string. `max_depth` controls how
    /// deeply nested a field access expression can be before it is considered an error.
    pub(crate) fn new(max_depth: usize, src: &'s str) -> Self {
        Self {
            max_depth,
            lexer: Lexer::new(src).peekable(),
        }
    }

    /// Entrypoint into the parser, parsing the root non-terminal -- `format`. Consumes all the
    /// remaining input in the parser and the parser itself.
    pub(crate) fn parse_format(mut self) -> Result<Vec<Strand<'s>>, Error> {
        let mut strands = vec![];
        while self.lexer.peek().is_some() {
            strands.push(self.parse_strand()?);
        }

        Ok(strands)
    }

    fn parse_strand(&mut self) -> Result<Strand<'s>, Error> {
        Ok(match_token! { self.lexer;
            L(T::Text | T::Escaped, _, _) => Strand::Text(self.parse_text()?),
            L(T::LCurl, _, _) => Strand::Expr(self.parse_expr()?),
        })
    }

    fn parse_text(&mut self) -> Result<Cow<'s, str>, Error> {
        let mut text = self.parse_part()?;
        while let Some(L(T::Text | T::Escaped, _, _)) = self.lexer.peek() {
            text += self.parse_part()?;
        }

        Ok(text)
    }

    fn parse_part(&mut self) -> Result<Cow<'s, str>, Error> {
        Ok(match_token! { self.lexer;
            L(T::Text | T::Escaped, _, slice) => {
                self.lexer.next();
                Cow::Borrowed(slice)
            }
        })
    }

    fn parse_expr(&mut self) -> Result<Vec<Element<'s>>, Error> {
        match_token! { self.lexer; L(T::LCurl, _, _) => self.lexer.next() };
        let mut idents = vec![self.parse_ident()?];

        loop {
            match_token! { self.lexer;
                L(T::RCurl, _, _) => {
                    self.lexer.next();
                    break;
                },
                L(T::Dot, off, _) => {
                    self.lexer.next();

                    if idents.len() >= self.max_depth {
                        return Err(Error::TooDeep {
                            max: self.max_depth,
                            off,
                        });
                    }

                    idents.push(self.parse_ident()?);
                }
            };
        }

        Ok(idents)
    }

    fn parse_ident(&mut self) -> Result<Element<'s>, Error> {
        Ok(match_token! { self.lexer;
            L(T::Ident, off, ident) => {
                self.lexer.next();
                if identifier::is_valid(ident) {
                    Element::Field(ident)
                } else {
                    return Err(Error::InvalidIdentifier { ident: ident.to_string(), off });
                }
            }
        })
    }
}

impl fmt::Display for Strand<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Strand::Text(text) => write!(f, "{text:?}"),
            Strand::Expr(path) => {
                let mut prefix = "";
                for field in path {
                    let Element::Field(name) = field else {
                        unreachable!("unexpected non-field element in path");
                    };

                    write!(f, "{prefix}{name}")?;
                    prefix = ".";
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(f: &str) -> Element<'_> {
        Element::Field(f)
    }

    #[test]
    fn test_literal_string() {
        assert_eq!(
            Parser::new(10, "foo bar").parse_format().unwrap(),
            vec![Strand::Text("foo bar".into())]
        );
    }

    #[test]
    fn test_field_expr() {
        assert_eq!(
            Parser::new(10, "{foo}").parse_format().unwrap(),
            vec![Strand::Expr(vec![field("foo")])]
        );
    }

    #[test]
    fn test_compound_expr() {
        assert_eq!(
            Parser::new(10, "{foo.bar.baz}").parse_format().unwrap(),
            vec![Strand::Expr(
                vec![field("foo"), field("bar"), field("baz"),]
            )]
        );
    }

    #[test]
    fn test_text_with_escape() {
        assert_eq!(
            Parser::new(10, r#"foo \{bar\} baz"#)
                .parse_format()
                .unwrap(),
            vec![Strand::Text(r#"foo {bar} baz"#.into())],
        );
    }

    #[test]
    fn test_escape_chain() {
        assert_eq!(
            Parser::new(10, r#"\\\\\\\\\"#).parse_format().unwrap(),
            vec![Strand::Text(r#"\\\\\"#.into())],
        );
    }

    #[test]
    fn test_back_to_back_exprs() {
        assert_eq!(
            Parser::new(10, "{foo . bar}{baz.qux}")
                .parse_format()
                .unwrap(),
            vec![
                Strand::Expr(vec![field("foo"), field("bar")]),
                Strand::Expr(vec![field("baz"), field("qux")])
            ]
        );
    }

    #[test]
    fn test_bad_identifier() {
        assert_eq!(
            Parser::new(10, "{foo.bar.baz!}")
                .parse_format()
                .unwrap_err()
                .to_string(),
            "Invalid identifier \"baz!\" at offset 9",
        );
    }

    #[test]
    fn test_unexpected_lcurly() {
        assert_eq!(
            Parser::new(10, "{foo{}}")
                .parse_format()
                .unwrap_err()
                .to_string(),
            "Unexpected '{' at offset 4, expected one of '}', or '.'",
        );
    }

    #[test]
    fn test_unexpected_rcurly() {
        assert_eq!(
            Parser::new(10, "foo bar}")
                .parse_format()
                .unwrap_err()
                .to_string(),
            "Unexpected '}' at offset 7, expected one of text, an escaped character, or '{'",
        );
    }

    #[test]
    fn test_no_dot() {
        assert_eq!(
            Parser::new(10, "{foo bar}")
                .parse_format()
                .unwrap_err()
                .to_string(),
            "Unexpected identifier \"bar\" at offset 5, expected one of '}', or '.'",
        );
    }

    #[test]
    fn test_empty_expr() {
        assert_eq!(
            Parser::new(10, "foo {} bar")
                .parse_format()
                .unwrap_err()
                .to_string(),
            "Unexpected '}' at offset 5, expected an identifier",
        );
    }

    #[test]
    fn test_unexpected_eos() {
        assert_eq!(
            Parser::new(10, "foo {bar")
                .parse_format()
                .unwrap_err()
                .to_string(),
            "Unexpected end-of-string, expected one of '}', or '.'",
        );
    }

    #[test]
    fn test_too_deep() {
        assert_eq!(
            Parser::new(2, "{foo.bar.baz}")
                .parse_format()
                .unwrap_err()
                .to_string(),
            "Field access at offset 8 deeper than the maximum of 2",
        );
    }
}
