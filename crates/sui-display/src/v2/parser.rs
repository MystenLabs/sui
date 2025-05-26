// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::{borrow::Cow, iter::Peekable};

use move_core_types::{
    account_address::AccountAddress,
    language_storage::{StructTag, TypeTag},
    u256::U256,
};

use super::lexer::{Lexeme as L, Lexer, OwnedLexeme, Token as T, TokenSet};

/// A single Display string template is a sequence of strands.
#[derive(Debug)]
pub enum Strand<'s> {
    /// Text strands are ported literally to the output.
    Text(Cow<'s, str>),

    /// Expr strands are interpreted to some value whose string representation is included in the
    /// output. They are surrounded by curly braces in the syntax, to differentiate them from text.
    Expr(Expr<'s>),
}

/// Expressions are composed of a number of alternates and an optional transform. During
/// evaluation, each alternate is evaluated in turn until the first one succeeds, and if a
/// transform is provided, it is applied to the result to convert it to a string.
#[derive(Debug)]
pub struct Expr<'s> {
    alternates: Vec<Chain<'s>>,
    transform: Option<&'s str>,
}

/// Chains are a sequence of nested field accesses.
#[derive(Debug)]
pub struct Chain<'s> {
    /// An optional root expression. If not provided, the object being displayed is the root.
    root: Option<Literal<'s>>,

    /// A sequence of field accessors that go successively deeper into the object.
    accessors: Vec<Accessor<'s>>,
}

/// Different ways to nest deeply into an object.
#[derive(Debug)]
pub enum Accessor<'s> {
    /// Access a named field.
    Field(&'s str),

    /// Index into a vector, VecMap, or dynamic field.
    Index(Chain<'s>),

    /// Index into a dynamic object field.
    IIndex(Chain<'s>),
}

/// Literal forms are elements whose syntax determines their (outer) type.
#[derive(Debug)]
pub enum Literal<'s> {
    // Primitives
    Address(AccountAddress),
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(U256),

    // Aggregates
    ByteArray(Vec<u8>),
    String(Cow<'s, str>),
    Vector(Box<Vector<'s>>),

    // Datatypes
    Struct(Box<Struct<'s>>),
    Enum(Box<Enum<'s>>),
}

/// Contents of a vector literal.
#[derive(Debug)]
pub struct Vector<'s> {
    /// Element type, optional for non-empty vectors.
    type_: Option<TypeTag>,
    elements: Vec<Chain<'s>>,
}

/// Contents of a struct literal.
#[derive(Debug)]
pub struct Struct<'s> {
    type_: StructTag,
    fields: Fields<'s>,
}

/// Contents of an enum literal.
#[derive(Debug)]
pub struct Enum<'s> {
    type_: StructTag,
    variant_name: Option<&'s str>,
    variant_index: u16,
    fields: Fields<'s>,
}

#[derive(Debug)]
pub enum Fields<'s> {
    Positional(Vec<Chain<'s>>),
    Named(Vec<(&'s str, Chain<'s>)>),
}

pub(crate) struct Parser<'s> {
    lexer: Peekable<Lexer<'s>>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
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

// Expression variant of `match_token!`, which evaluates to `Some(...)` if an arm matches,
// consuming the next token, or `None` without consuming if there is no next token, or it doesn't
// match any of the provided patterns.
macro_rules! match_token_opt {
    ($lexer:expr; $(L($($pat:path)|+, $off:pat, $slice:pat) => $expr:expr),+ $(,)?) => {{
        match $lexer.peek() {
            $(Some(&L($($pat)|+, $off, $slice)) => Some($expr),)+
            Some(_) | None => None
        }
    }};
}

/// Recursive descent parser for Display V2 format strings, parsing the following grammar:
///
///   format   ::= strand*
///
///   strand   ::= text | expr
///
///   text     ::= part+
///
///   part     ::= TEXT | '{{' | '}}'
///
///   expr     ::= '{' chain ('|' chain)* (':' IDENT)? '}'
///
///   chain    ::= (literal | IDENT) accessor*
///
///   accessor ::= '.' IDENT
///              | '[' chain ']'
///              | '[' '[' chain ']' ']'
///
///   literal  ::= address | bool | number | string | vector | struct | enum
///
///   address  ::= '@' NUM_HEX
///
///   bool     ::= 'true' | 'false'
///
///   number   ::= (NUM_DEC | NUM_HEX) numeric?
///
///   string   ::= ('b' | 'x')? STRING
///
///   vector   ::= 'vector'  '<' type '>'
///              | 'vector' ('<' type '>')? '[' chain (',' chain)* ','? ']'
///
///   struct   ::= datatype fields
///
///   enum     ::= datatype '::' (IDENT '#')? NUM_DEC fields
///
///   fields   ::= '(' chain (',' chain)* ','? ')'
///              | '{' named (',' named)* ','? '}'
///
///   named    ::= IDENT ':' chain
///
///   type     ::= 'address' | 'bool' | | 'vector' '<' type '>' |  numtype | datatype
///
///   datatype ::= NUM_HEX '::' IDENT ('<' type (',' type)* ','? '>')?
///
///   numeric  ::= 'u8' | 'u16' | 'u32' | 'u64' | 'u128' | 'u256'
///
impl<'s> Parser<'s> {
    /// Construct a new parser, consuming input from the `src` string.
    pub(crate) fn new(src: &'s str) -> Self {
        Self {
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
            L(T::Text | T::LLBrace | T::RRBrace, _, _) => Strand::Text(self.parse_text()?),
            L(T::LBrace, _, _) => Strand::Expr(self.parse_expr()?),
        })
    }

    fn parse_text(&mut self) -> Result<Cow<'s, str>, Error> {
        let mut text = self.parse_part()?;
        while let Some(L(T::Text | T::LLBrace | T::RRBrace, _, _)) = self.lexer.peek() {
            text += self.parse_part()?;
        }

        Ok(text)
    }

    fn parse_part(&mut self) -> Result<Cow<'s, str>, Error> {
        Ok(match_token! { self.lexer;
            L(T::Text | T::LLBrace | T::RRBrace, _, slice) => {
                self.lexer.next();
                Cow::Borrowed(slice)
            }
        })
    }

    fn parse_expr(&mut self) -> Result<Expr<'s>, Error> {
        match_token! { self.lexer; L(T::LBrace, _, _) => self.lexer.next() };
        let mut alternates = vec![self.parse_chain()?];
        let mut transform = None;

        loop {
            self.eat_whitespace();
            match_token! { self.lexer;
                L(T::RBrace, _, _) => {
                    self.lexer.next();
                    break;
                },

                L(T::Colon, _, _) => {
                    self.lexer.next();
                    self.eat_whitespace();
                    match_token! { self.lexer; L(T::Ident, _, t) => {
                        self.lexer.next();
                        transform = Some(t);
                    }};
                    self.eat_whitespace();
                    match_token! { self.lexer; L(T::RBrace, _, _) => {
                        self.lexer.next()
                    }};
                    break;
                },

                L(T::Pipe, _, _) => {
                    self.lexer.next();
                    alternates.push(self.parse_chain()?);
                }
            }
        }

        Ok(Expr {
            alternates,
            transform,
        })
    }

    fn parse_chain(&mut self) -> Result<Chain<'s>, Error> {
        let mut accessors = vec![];
        let root = self.try_parse_literal()?;

        // If there is no root literal, the chain must start with an identifier, representing a
        // field on the object being displayed.
        if root.is_none() {
            accessors.push(match_token! { self.lexer; L(T::Ident, _, i) => {
                self.lexer.next();
                Accessor::Field(i)
            }})
        }

        while let Some(accessor) = self.try_parse_accessor()? {
            accessors.push(accessor);
        }

        Ok(Chain { root, accessors })
    }

    fn try_parse_literal(&mut self) -> Result<Option<Literal<'s>>, Error> {
        self.eat_whitespace();
        Ok(match_token_opt! { self.lexer;
            L(T::Ident, _, "true") => {
                self.lexer.next();
                Literal::Bool(true)
            },
        })
    }

    fn try_parse_accessor(&mut self) -> Result<Option<Accessor<'s>>, Error> {
        self.eat_whitespace();
        Ok(match_token_opt! { self.lexer;
            L(T::Dot, _, _) => {
                self.lexer.next();
                self.eat_whitespace();
                match_token! { self.lexer; L(T::Ident, _, f) => {
                    self.lexer.next();
                    Accessor::Field(f)
                }}
            },

            L(T::LBracket, _, _) => {
                self.lexer.next();
                if matches!(self.lexer.peek(), Some(L(T::LBracket, _, _))) {
                    self.lexer.next();
                    let chain = self.parse_chain()?;
                    self.eat_whitespace();
                    match_token! { self.lexer; L(T::RBracket, _, _) => self.lexer.next() };
                    match_token! { self.lexer; L(T::RBracket, _, _) => self.lexer.next() };
                    Accessor::IIndex(chain)
                } else {
                    let chain = self.parse_chain()?;
                    self.eat_whitespace();
                    match_token! { self.lexer; L(T::RBracket, _, _) => self.lexer.next() };
                    Accessor::Index(chain)
                }
            },
        })
    }

    fn eat_whitespace(&mut self) {
        // The lexer merges together consecutive whitespace tokens, so if one is found, there is no
        // need to check for more.
        if let Some(L(T::Whitespace, _, _)) = self.lexer.peek() {
            self.lexer.next();
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;

    fn strands(src: &str) -> String {
        let strands = match Parser::new(src).parse_format() {
            Ok(strands) => strands,
            Err(e) => return format!("Error: {e}"),
        };

        let mut output = String::new();
        for strand in strands {
            output += &format!("{strand:#?}\n");
        }

        output
    }

    #[test]
    fn test_all_text() {
        assert_snapshot!(strands(r#"foo bar"#), @r###"
        Text(
            "foo bar",
        )
        "###);
    }

    #[test]
    fn test_field_expr() {
        assert_snapshot!(strands(r#"{foo}"#), @r###"
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "foo",
                            ),
                        ],
                    },
                ],
                transform: None,
            },
        )
        "###);
    }

    #[test]
    fn test_literal_expr() {
        assert_snapshot!(strands(r#"{true}"#), @r###"
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: Some(
                            Bool(
                                true,
                            ),
                        ),
                        accessors: [],
                    },
                ],
                transform: None,
            },
        )
        "###);
    }

    #[test]
    fn test_nested_field_expr() {
        assert_snapshot!(strands(r#"{foo.bar.baz}"#), @r###"
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "foo",
                            ),
                            Field(
                                "bar",
                            ),
                            Field(
                                "baz",
                            ),
                        ],
                    },
                ],
                transform: None,
            },
        )
        "###);
    }

    #[test]
    fn test_text_with_escapes() {
        assert_snapshot!(strands(r#"foo {{bar}} baz"#), @r###"
        Text(
            "foo {bar} baz",
        )
        "###);
    }

    #[test]
    fn test_back_to_back_exprs() {
        assert_snapshot!(strands(r#"{foo . bar}{baz.qux}"#), @r###"
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "foo",
                            ),
                            Field(
                                "bar",
                            ),
                        ],
                    },
                ],
                transform: None,
            },
        )
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "baz",
                            ),
                            Field(
                                "qux",
                            ),
                        ],
                    },
                ],
                transform: None,
            },
        )
        "###);
    }

    #[test]
    fn test_triple_curlies() {
        assert_snapshot!(strands(r#"foo {{{bar} {baz}}}"#), @r###"
        Text(
            "foo {",
        )
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "bar",
                            ),
                        ],
                    },
                ],
                transform: None,
            },
        )
        Text(
            " ",
        )
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "baz",
                            ),
                        ],
                    },
                ],
                transform: None,
            },
        )
        Text(
            "}",
        )
        "###);
    }

    #[test]
    fn test_alternates() {
        assert_snapshot!(strands(r#"{foo | bar | baz}"#), @r###"
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "foo",
                            ),
                        ],
                    },
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "bar",
                            ),
                        ],
                    },
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "baz",
                            ),
                        ],
                    },
                ],
                transform: None,
            },
        )
        "###);
    }

    #[test]
    fn test_alternates_with_transform() {
        assert_snapshot!(strands(r#"{foo | bar | baz :base64}"#), @r###"
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "foo",
                            ),
                        ],
                    },
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "bar",
                            ),
                        ],
                    },
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "baz",
                            ),
                        ],
                    },
                ],
                transform: Some(
                    "base64",
                ),
            },
        )
        "###);
    }

    #[test]
    fn test_expr_with_transform() {
        assert_snapshot!(strands(r#"{foo.bar.baz:url}"#), @r###"
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "foo",
                            ),
                            Field(
                                "bar",
                            ),
                            Field(
                                "baz",
                            ),
                        ],
                    },
                ],
                transform: Some(
                    "url",
                ),
            },
        )
        "###);
    }

    #[test]
    fn test_index_chain() {
        assert_snapshot!(strands(r#"{foo[bar][[baz]].qux[quy]}"#), @r###"
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: None,
                        accessors: [
                            Field(
                                "foo",
                            ),
                            Index(
                                Chain {
                                    root: None,
                                    accessors: [
                                        Field(
                                            "bar",
                                        ),
                                    ],
                                },
                            ),
                            IIndex(
                                Chain {
                                    root: None,
                                    accessors: [
                                        Field(
                                            "baz",
                                        ),
                                    ],
                                },
                            ),
                            Field(
                                "qux",
                            ),
                            Index(
                                Chain {
                                    root: None,
                                    accessors: [
                                        Field(
                                            "quy",
                                        ),
                                    ],
                                },
                            ),
                        ],
                    },
                ],
                transform: None,
            },
        )
        "###);
    }

    #[test]
    fn test_index_with_root() {
        assert_snapshot!(strands(r#"{true[[foo]][bar].baz}"#), @r###"
        Expr(
            Expr {
                alternates: [
                    Chain {
                        root: Some(
                            Bool(
                                true,
                            ),
                        ),
                        accessors: [
                            IIndex(
                                Chain {
                                    root: None,
                                    accessors: [
                                        Field(
                                            "foo",
                                        ),
                                    ],
                                },
                            ),
                            Index(
                                Chain {
                                    root: None,
                                    accessors: [
                                        Field(
                                            "bar",
                                        ),
                                    ],
                                },
                            ),
                            Field(
                                "baz",
                            ),
                        ],
                    },
                ],
                transform: None,
            },
        )
        "###);
    }

    #[test]
    fn test_unbalanced_curlies() {
        assert_snapshot!(strands(r#"{foo"#), @"Error: Unexpected end-of-string, expected one of '}', ':', or '|'");
    }

    #[test]
    fn test_missing_field_identifier() {
        assert_snapshot!(strands(r#"{foo..bar}"#), @"Error: Unexpected '.' at offset 5, expected an identifier");
    }

    #[test]
    fn test_unbalanced_index() {
        assert_snapshot!(strands(r#"{foo[bar}"#), @"Error: Unexpected '}' at offset 8, expected ']'");
    }

    #[test]
    fn test_spaced_out_left_double_index() {
        assert_snapshot!(strands(r#"{foo[ [bar]]}"#), @"Error: Unexpected '[' at offset 6, expected an identifier");
    }

    #[test]
    fn test_spaced_out_right_double_index() {
        assert_snapshot!(strands(r#"{foo[[bar] ]}"#), @"Error: Unexpected whitespace at offset 10, expected ']'");
    }

    #[test]
    fn test_unbalanced_double_index() {
        assert_snapshot!(strands(r#"{foo[[bar}"#), @"Error: Unexpected '}' at offset 9, expected ']'");
    }

    #[test]
    fn test_triple_index() {
        assert_snapshot!(strands(r#"{foo[[[bar]]]}"#), @"Error: Unexpected '[' at offset 6, expected an identifier");
    }

    #[test]
    fn test_unexpected_characters() {
        assert_snapshot!(strands(r#"anything goes {? % ! 🔥}"#), @r###"Error: Unexpected "?" at offset 15, expected an identifier"###);
    }
}
