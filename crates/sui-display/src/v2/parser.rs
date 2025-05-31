// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::borrow::Cow;

use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    u256::U256,
};

use super::error::{Error, Expected, ExpectedSet, Match};
use super::lexer::{Lexeme as Lex, Lexer, Token as T};
use super::peek::{Peekable2, Peekable2Ext};

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
    lexer: Peekable2<Lexer<'s>>,
}

/// Helper macro for constructing an `Expected` enum variant based on the kind of pattern being
/// matched on in the parser. The first argument is the kind, which denotes whether the pattern was
/// looking for a token, regardless of the contents of the underlying slice, or a literal match on
/// a particular string slice.
macro_rules! expected {
    (Lit, $pat:path, $slice:tt) => {
        Expected::Literal($slice)
    };

    (Tok, $pat:path, $slice:tt) => {
        Expected::Token($pat)
    };
}

/// Pattern match on the next token in the lexer, without consuming it. Returns an error if there
/// is no next token, or if the next token doesn't match any of the provided patterns. The error
/// enumerates all the tokens that were expected, including the tokens that were checked
/// `$prev`iously, if any were provided.
macro_rules! match_token {
    (
        $lexer:expr $(, $prev:expr)?;
        $($kind:ident($($pat:path)|+, $off:pat, $slice:tt) $(if $cond:expr)? => $expr:expr),+
        $(,)?
    ) => {{
        match $lexer.peek() {
            $(Some(&Lex($($pat)|+, $off, $slice)) $(if $cond)? => $expr,)+
            lexeme => return Err(ExpectedSet::new(&[$($(expected!($kind, $pat, $slice)),+),+])
                $(.with_prev($prev))?
                .into_error(lexeme)),
        }
    }};
}

/// Expression variant of `match_token!`, which evaluates to `Match::Found(...)` if an arm matches,
/// consuming the next token, or `Match::Tried(...)` without consuming if there is no next token,
/// or it doesn't match any of the provided patterns.
///
/// In the latter case, the set of patterns checked is included in the output, so that the parser
/// can accumulate all the patterns it has tried to match against at the current position.
///
/// The optional `$prev` argument can be used to include the set of patterns that were checked
/// before this match.
macro_rules! match_token_opt {
    (
        $lexer:expr $(, $prev:expr)?;
        $($kind:ident($($pat:path)|+, $off:pat, $slice:tt) $(if $cond:expr)? => $expr:expr),+
        $(,)?
    ) => {{
        match $lexer.peek() {
            $(Some(&Lex($($pat)|+, $off, $slice)) $(if $cond)? => Match::Found($expr),)+
            lexeme => Match::Tried(
                lexeme.map(|l| l.1),
                ExpectedSet::new(&[$($(expected!($kind, $pat, $slice)),+),+])
                    $(.with_prev($prev))?
            ),
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
///   vector   ::= 'vector'  '<' type ','? '>'
///              | 'vector' ('<' type ','? '>')?  array
///
///   array    ::= '[' chain (',' chain)* ','? ']'
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
            lexer: Lexer::new(src).peekable2(),
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
            Tok(T::Text | T::LLBrace | T::RRBrace, _, _) => Strand::Text(self.parse_text()?),
            Tok(T::LBrace, _, _) => Strand::Expr(self.parse_expr()?),
        })
    }

    fn parse_text(&mut self) -> Result<Cow<'s, str>, Error> {
        let mut text = self.parse_part()?;
        while let Some(Lex(T::Text | T::LLBrace | T::RRBrace, _, _)) = self.lexer.peek() {
            text += self.parse_part()?;
        }

        Ok(text)
    }

    fn parse_part(&mut self) -> Result<Cow<'s, str>, Error> {
        Ok(match_token! { self.lexer;
            Tok(T::Text | T::LLBrace | T::RRBrace, _, slice) => {
                self.lexer.next();
                Cow::Borrowed(slice)
            }
        })
    }

    fn parse_expr(&mut self) -> Result<Expr<'s>, Error> {
        match_token! { self.lexer; Tok(T::LBrace, _, _) => self.lexer.next() };
        self.eat_whitespace();
        let mut alternates = vec![self.parse_chain()?];
        let mut transform = None;

        loop {
            self.eat_whitespace();
            match_token! { self.lexer;
                Tok(T::RBrace, _, _) => {
                    self.lexer.next();
                    break;
                },

                Tok(T::Colon, _, _) => {
                    self.lexer.next();
                    self.eat_whitespace();
                    match_token! { self.lexer; Tok(T::Ident, _, t) => {
                        self.lexer.next();
                        transform = Some(t);
                    }};
                    self.eat_whitespace();
                    match_token! { self.lexer; Tok(T::RBrace, _, _) => {
                        self.lexer.next()
                    }};
                    break;
                },

                Tok(T::Pipe, _, _) => {
                    self.lexer.next();
                    self.eat_whitespace();
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

        // If there is no root literal, the chain must start with an identifier, representing a
        // field on the object being displayed.
        let root = match self.try_parse_literal()? {
            Match::Found(literal) => Some(literal),
            Match::Tried(_, tried) => {
                accessors.push(match_token! { self.lexer, tried; Tok(T::Ident, _, i) => {
                    self.lexer.next();
                    Accessor::Field(i)
                }});
                None
            }
        };

        while let Match::Found(accessor) = {
            self.eat_whitespace();
            self.try_parse_accessor()?
        } {
            accessors.push(accessor);
        }

        Ok(Chain { root, accessors })
    }

    fn try_parse_literal(&mut self) -> Result<Match<Literal<'s>>, Error> {
        Ok(match_token_opt! { self.lexer;
            Tok(T::At, _, _) => {
                self.lexer.next();
                Literal::Address(self.parse_address()?)
            },

            Lit(T::Ident, _, "true") => {
                self.lexer.next();
                Literal::Bool(true)
            },

            Lit(T::Ident, _, "false") => {
                self.lexer.next();
                Literal::Bool(false)
            },

            Tok(T::NumDec | T::NumHex, _, _) if self.lexer.peek2().is_some_and(|Lex(t, _, _)| *t == T::CColon) => {
                self.parse_data()?
            },

            Tok(T::NumDec, _, dec) => {
                self.lexer.next();
                self.parse_numeric_suffix(dec, 10)?
            },

            Tok(T::NumHex, _, hex) => {
                self.lexer.next();
                self.parse_numeric_suffix(hex, 16)?
            },

            Tok(T::String, _, slice) => {
                self.lexer.next();
                Literal::String(read_string_literal(slice))
            },

            Lit(T::Ident, _, "b") if self.lexer.peek2().is_some_and(|Lex(t, _, _)| *t == T::String) => {
                self.lexer.next();

                // SAFETY: Match guard peeks ahead to this token.
                let Lex(_, _, slice) = self.lexer.next().unwrap();
                let output = read_string_literal(slice);
                Literal::ByteArray(output.into_owned().into_bytes())
            },

            Lit(T::Ident, _, "x") if self.lexer.peek2().is_some_and(|Lex(t, _, _)| *t == T::String) => {
                self.lexer.next();

                // SAFETY: Match guard peeks ahead to this token.
                let lex @ Lex(_, _, slice) = self.lexer.next().unwrap();
                let output = read_hex_literal(&lex, slice)?;
                Literal::ByteArray(output)
            },

            Tok(T::Ident, offset, "vector") => {
                self.lexer.next();

                self.eat_whitespace();
                let mut type_params = self.parse_type_params()?;
                let type_ = match type_params.len() {
                    // SAFETY: Bounds check on `type_params.len()` guarantees safety.
                    1 => Some(type_params.pop().unwrap()),
                    0 => None,
                    arity => return Err(Error::VectorArity { offset, arity }),
                };

                self.eat_whitespace();
                let elements = self.parse_array_elements()?;

                // If the vector is empty, the type parameter becomes mandatory.
                if elements.is_empty() && type_.is_none() {
                    return Err(Error::VectorArity {
                        offset,
                        arity: 0,
                    });
                }

                Literal::Vector(Box::new(Vector { type_, elements }))
            },
        })
    }

    fn try_parse_accessor(&mut self) -> Result<Match<Accessor<'s>>, Error> {
        Ok(match_token_opt! { self.lexer;
            Tok(T::Dot, _, _) => {
                self.lexer.next();
                self.eat_whitespace();
                match_token! { self.lexer; Tok(T::Ident, _, f) => {
                    self.lexer.next();
                    Accessor::Field(f)
                }}
            },

            Tok(T::LBracket, _, _) => {
                self.lexer.next();
                if matches!(self.lexer.peek(), Some(Lex(T::LBracket, _, _))) {
                    self.lexer.next();
                    self.eat_whitespace();
                    let chain = self.parse_chain()?;
                    self.eat_whitespace();
                    match_token! { self.lexer; Tok(T::RBracket, _, _) => self.lexer.next() };
                    match_token! { self.lexer; Tok(T::RBracket, _, _) => self.lexer.next() };
                    Accessor::IIndex(chain)
                } else {
                    self.eat_whitespace();
                    let chain = self.parse_chain()?;
                    self.eat_whitespace();
                    match_token! { self.lexer; Tok(T::RBracket, _, _) => self.lexer.next() };
                    Accessor::Index(chain)
                }
            },
        })
    }

    fn parse_array_elements(&mut self) -> Result<Vec<Chain<'s>>, Error> {
        let mut elements = vec![];

        if match_token_opt! { self.lexer; Tok(T::LBracket, _, _) => { self.lexer.next(); } }
            .is_not_found()
        {
            return Ok(elements);
        }

        self.eat_whitespace();
        let terminated = match_token_opt! { self.lexer;
            Tok(T::RBracket, _, _) => { self.lexer.next(); }
        };

        let (offset, terminated) = match terminated {
            Match::Tried(offset, terminated) => (offset, terminated),
            Match::Found(_) => return Ok(elements),
        };

        loop {
            self.eat_whitespace();
            elements.push(
                self.parse_chain()
                    .map_err(|e| e.also_tried(offset, terminated.clone()))?,
            );

            self.eat_whitespace();
            let delimited = match_token_opt! { self.lexer;
                Tok(T::Comma, _, _) => { self.lexer.next(); }
            };

            self.eat_whitespace();
            let terminated = match_token_opt! { self.lexer;
                Tok(T::RBracket, _, _) => { self.lexer.next(); }
            };

            match (delimited, terminated) {
                (_, Match::Found(_)) => break,
                (Match::Found(_), _) => continue,
                (Match::Tried(_, delimited), Match::Tried(_, terminated)) => {
                    return Err(delimited.union(terminated).into_error(self.lexer.peek()))
                }
            }
        }

        Ok(elements)
    }

    fn parse_data(&mut self) -> Result<Literal<'s>, Error> {
        let type_ = self.parse_datatype()?;

        let enum_ = match_token_opt! { self.lexer;
            Tok(T::CColon, _, _) => { self.lexer.next(); }
        };

        if let Match::Tried(offset, enum_) = enum_ {
            self.eat_whitespace();
            return Ok(Literal::Struct(Box::new(Struct {
                type_,
                fields: self
                    .parse_fields()
                    .map_err(|e| e.also_tried(offset, enum_))?,
            })));
        }

        Ok(match_token! { self.lexer;
            Tok(T::Ident, _, variant_name) => {
                self.lexer.next();

                match_token! { self.lexer; Tok(T::Pound, _, _) => self.lexer.next() };
                let variant_index = match_token! { self.lexer; Tok(T::NumDec, _, index) => {
                    self.lexer.next();
                    read_u16(index, 10, "enum variant index")?
                }};

                self.eat_whitespace();
                Literal::Enum(Box::new(Enum {
                    type_,
                    variant_name: Some(variant_name),
                    variant_index,
                    fields: self.parse_fields()?,
                }))
            },

            Tok(T::NumDec, _, index) => {
                self.lexer.next();

                let variant_index = read_u16(index, 10, "enum variant index")?;

                self.eat_whitespace();
                Literal::Enum(Box::new(Enum {
                    type_,
                    variant_name: None,
                    variant_index,
                    fields: self.parse_fields()?,
                }))
            }
        })
    }

    fn parse_fields(&mut self) -> Result<Fields<'s>, Error> {
        let is_named = match_token! { self.lexer;
            Tok(T::LParen, _, _) => { self.lexer.next(); false },
            Tok(T::LBrace, _, _) => { self.lexer.next(); true }
        };

        self.eat_whitespace();
        Ok(if is_named {
            let mut fields = vec![];

            let terminated = match_token_opt! { self.lexer;
                Tok(T::RBrace, _, _) => { self.lexer.next(); }
            };

            let terminated = match terminated {
                Match::Tried(_, terminated) => terminated,
                Match::Found(_) => return Ok(Fields::Named(fields)),
            };

            loop {
                self.eat_whitespace();
                let name = match_token! { self.lexer, terminated;
                    Tok(T::Ident, _, n) => { self.lexer.next(); n }
                };

                self.eat_whitespace();
                match_token! { self.lexer; Tok(T::Colon, _, _) => self.lexer.next() };

                self.eat_whitespace();
                let value = self.parse_chain()?;

                fields.push((name, value));

                self.eat_whitespace();
                let delimited = match_token_opt! { self.lexer;
                    Tok(T::Comma, _, _) => { self.lexer.next(); }
                };

                self.eat_whitespace();
                let terminated = match_token_opt! { self.lexer;
                    Tok(T::RBrace, _, _) => { self.lexer.next(); }
                };

                match (delimited, terminated) {
                    (_, Match::Found(_)) => break,
                    (Match::Found(_), _) => continue,
                    (Match::Tried(_, delimited), Match::Tried(_, terminated)) => {
                        return Err(delimited.union(terminated).into_error(self.lexer.peek()))
                    }
                }
            }

            Fields::Named(fields)
        } else {
            let mut fields = vec![];

            let terminated = match_token_opt! { self.lexer;
                Tok(T::RParen, _, _) => { self.lexer.next(); }
            };

            let (offset, terminated) = match terminated {
                Match::Tried(offset, terminated) => (offset, terminated),
                Match::Found(_) => return Ok(Fields::Positional(fields)),
            };

            loop {
                self.eat_whitespace();
                fields.push(
                    self.parse_chain()
                        .map_err(|e| e.also_tried(offset, terminated.clone()))?,
                );

                self.eat_whitespace();
                let delimited = match_token_opt! { self.lexer;
                    Tok(T::Comma, _, _) => { self.lexer.next(); }
                };

                self.eat_whitespace();
                let terminated = match_token_opt! { self.lexer;
                    Tok(T::RParen, _, _) => { self.lexer.next(); }
                };

                match (delimited, terminated) {
                    (_, Match::Found(_)) => break,
                    (Match::Found(_), _) => continue,
                    (Match::Tried(_, delimited), Match::Tried(_, terminated)) => {
                        return Err(delimited.union(terminated).into_error(self.lexer.peek()))
                    }
                }
            }

            Fields::Positional(fields)
        })
    }

    fn parse_type(&mut self) -> Result<TypeTag, Error> {
        Ok(match_token! { self.lexer;
            Lit(T::Ident, _, "address") => { self.lexer.next(); TypeTag::Address },
            Lit(T::Ident, _, "bool") => { self.lexer.next(); TypeTag::Bool },
            Lit(T::Ident, _, "u8") => { self.lexer.next(); TypeTag::U8 },
            Lit(T::Ident, _, "u16") => { self.lexer.next(); TypeTag::U16 },
            Lit(T::Ident, _, "u32") => { self.lexer.next(); TypeTag::U32 },
            Lit(T::Ident, _, "u64") => { self.lexer.next(); TypeTag::U64 },
            Lit(T::Ident, _, "u128") => { self.lexer.next(); TypeTag::U128 },
            Lit(T::Ident, _, "u256") => { self.lexer.next(); TypeTag::U256 },

            Lit(T::Ident, offset, "vector") => {
                self.lexer.next();
                self.eat_whitespace();
                let mut type_params = self.parse_type_params()?;
                match type_params.len() {
                    // SAFETY: Bounds check on `type_params.len()` guarantees safety.
                    1 => TypeTag::Vector(Box::new(type_params.pop().unwrap())),
                    arity => return Err(Error::VectorArity { offset, arity }),
                }
            },

            Tok(T::NumDec | T::NumHex, _, _) => {
                TypeTag::Struct(Box::new(self.parse_datatype()?))
            }
        })
    }

    fn parse_datatype(&mut self) -> Result<StructTag, Error> {
        let address = self.parse_address()?;

        match_token! { self.lexer; Tok(T::CColon, _, _) => self.lexer.next() };
        let module = self.parse_identifier()?;

        match_token! { self.lexer; Tok(T::CColon, _, _) => self.lexer.next() };
        let name = self.parse_identifier()?;

        self.eat_whitespace();
        let type_params = self.parse_type_params()?;

        Ok(StructTag {
            address,
            module,
            name,
            type_params,
        })
    }

    fn parse_type_params(&mut self) -> Result<Vec<TypeTag>, Error> {
        let mut type_params = vec![];
        if match_token_opt! { self.lexer; Tok(T::LAngle, _, _) => { self.lexer.next(); } }
            .is_not_found()
        {
            return Ok(type_params);
        }

        loop {
            self.eat_whitespace();
            type_params.push(self.parse_type()?);

            self.eat_whitespace();
            let delimited = match_token_opt! { self.lexer;
                Tok(T::Comma, _, _) => { self.lexer.next(); }
            };

            self.eat_whitespace();
            let terminated = match_token_opt! { self.lexer;
                Tok(T::RAngle, _, _) => { self.lexer.next(); }
            };

            match (delimited, terminated) {
                (_, Match::Found(_)) => break,
                (Match::Found(_), _) => continue,
                (Match::Tried(_, delimited), Match::Tried(_, terminated)) => {
                    return Err(delimited.union(terminated).into_error(self.lexer.peek()))
                }
            }
        }

        Ok(type_params)
    }

    fn parse_address(&mut self) -> Result<AccountAddress, Error> {
        let addr = match_token! { self.lexer;
            Tok(T::NumHex, _, slice) => {
                self.lexer.next();
                read_u256(slice, 16, "'address'")?
            },

            Tok(T::NumDec, _, slice) => {
                self.lexer.next();
                read_u256(slice, 10, "'address'")?
            },
        };

        Ok(AccountAddress::from(addr.to_be_bytes()))
    }

    fn parse_identifier(&mut self) -> Result<Identifier, Error> {
        let lex @ Lex(_, _, ident) = match_token! { self.lexer;
            Tok(T::Ident, _, _) => {
                // SAFETY: Inside a match token arm, so there is guaranteed to be a next token.
                self.lexer.next().unwrap()
            },
        };

        Identifier::new(ident).map_err(|_| Error::InvalidIdentifier(lex.detach()))
    }

    fn parse_numeric_suffix(&mut self, num: &'s str, radix: u32) -> Result<Literal<'s>, Error> {
        let literal = match_token_opt! { self.lexer;
            Lit(T::Ident, _, "u8") => { self.lexer.next(); Literal::U8(read_u8(num, radix, "'u8'")?) },
            Lit(T::Ident, _, "u16") => { self.lexer.next(); Literal::U16(read_u16(num, radix, "'u16'")?) },
            Lit(T::Ident, _, "u32") => { self.lexer.next(); Literal::U32(read_u32(num, radix, "'u32'")?) },
            Lit(T::Ident, _, "u64") => { self.lexer.next(); Literal::U64(read_u64(num, radix, "'u64'")?) },
            Lit(T::Ident, _, "u128") => { self.lexer.next(); Literal::U128(read_u128(num, radix, "'u128'")?) },
            Lit(T::Ident, _, "u256") => { self.lexer.next(); Literal::U256(read_u256(num, radix, "'u256'")?) },
        };

        // If there was no explicit type suffix, assume `u64`.
        Ok(if let Match::Found(lit) = literal {
            lit
        } else {
            Literal::U64(read_u64(num, radix, "'u64'")?)
        })
    }

    fn eat_whitespace(&mut self) {
        // The lexer merges together consecutive whitespace tokens, so if one is found, there is no
        // need to check for more.
        if let Some(Lex(T::Whitespace, _, _)) = self.lexer.peek() {
            self.lexer.next();
        }
    }
}

fn read_u8(slice: &str, radix: u32, what: &'static str) -> Result<u8, Error> {
    u8::from_str_radix(&slice.replace('_', ""), radix).map_err(|_| Error::NumberOverflow { what })
}

fn read_u16(slice: &str, radix: u32, what: &'static str) -> Result<u16, Error> {
    u16::from_str_radix(&slice.replace('_', ""), radix).map_err(|_| Error::NumberOverflow { what })
}

fn read_u32(slice: &str, radix: u32, what: &'static str) -> Result<u32, Error> {
    u32::from_str_radix(&slice.replace('_', ""), radix).map_err(|_| Error::NumberOverflow { what })
}

fn read_u64(slice: &str, radix: u32, what: &'static str) -> Result<u64, Error> {
    u64::from_str_radix(&slice.replace('_', ""), radix).map_err(|_| Error::NumberOverflow { what })
}

fn read_u128(slice: &str, radix: u32, what: &'static str) -> Result<u128, Error> {
    u128::from_str_radix(&slice.replace('_', ""), radix).map_err(|_| Error::NumberOverflow { what })
}

fn read_u256(slice: &str, radix: u32, what: &'static str) -> Result<U256, Error> {
    U256::from_str_radix(&slice.replace('_', ""), radix).map_err(|_| Error::NumberOverflow { what })
}

fn read_string_literal(slice: &str) -> Cow<'_, str> {
    let mut start = slice.find('\\').unwrap_or(slice.len());
    let mut output = Cow::Borrowed(&slice[0..start]);

    while start < slice.len() {
        // Skip the escape character.
        start += 1;

        // Slurp up to the next escape character, or the end of the string.
        let end = slice[start + 1..]
            .find('\\')
            .map_or(slice.len(), |i| start + 1 + i);

        output += &slice[start..end];
        start = end;
    }

    output
}

fn read_hex_literal(lexeme: &Lex<'_>, slice: &str) -> Result<Vec<u8>, Error> {
    if slice.len() % 2 != 0 {
        return Err(Error::OddHexLiteral(lexeme.detach()));
    }

    let mut output = Vec::with_capacity(slice.len() / 2);
    for i in (0..slice.len()).step_by(2) {
        let byte = u8::from_str_radix(&slice[i..i + 2], 16)
            .map_err(|_| Error::InvalidHexCharacter(lexeme.detach()))?;
        output.push(byte);
    }

    Ok(output)
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
        assert_snapshot!(strands(r#"foo bar"#))
    }

    #[test]
    fn test_field_expr() {
        assert_snapshot!(strands(r#"{foo}"#));
    }

    #[test]
    fn test_pseudo_keyword_expr() {
        // Certain identifiers are keywords if they are followed by some tokens, but otherwise not.
        assert_snapshot!(strands(r#"{b | x}"#));
    }

    #[test]
    fn test_literal_expr() {
        assert_snapshot!(strands(r#"{true}"#));
    }

    #[test]
    fn test_nested_field_expr() {
        assert_snapshot!(strands(r#"{foo.bar.baz}"#));
    }

    #[test]
    fn test_text_with_escapes() {
        assert_snapshot!(strands(r#"foo {{bar}} baz"#));
    }

    #[test]
    fn test_back_to_back_exprs() {
        assert_snapshot!(strands(r#"{foo . bar}{baz.qux}"#));
    }

    #[test]
    fn test_triple_curlies() {
        assert_snapshot!(strands(r#"foo {{{bar} {baz}}}"#));
    }

    #[test]
    fn test_alternates() {
        assert_snapshot!(strands(r#"{foo | bar | baz}"#));
    }

    #[test]
    fn test_alternates_with_transform() {
        assert_snapshot!(strands(r#"{foo | bar | baz :base64}"#));
    }

    #[test]
    fn test_expr_with_transform() {
        assert_snapshot!(strands(r#"{foo.bar.baz:url}"#));
    }

    #[test]
    fn test_address_literal() {
        assert_snapshot!(strands(r#"{@0x1 | @42 | @0x12_34_56}"#));
    }

    #[test]
    fn test_bool_literals() {
        assert_snapshot!(strands(r#"{true | false}"#));
    }

    #[test]
    fn test_numeric_literals() {
        assert_snapshot!(strands(
            "Decimal Literals: \
            { 42 | 42u8 | 1_234u16 | 56_789_012_345u64 | 678_901_234_567_890_123_456u128 | 7_890_123_456_789_012_345_678_901_234_567_890_123_456u256 } \
            Hexadecimal literals: \
            { 0x42 | 0x42u8 | 0x123u16 | 0x4_5678u32 | 0x90_1234_5678u64 | 0x90_1234_5678_9012_3456u128 | 0x78_9012_3456_7890_1234_5679_0123_4567_8901u256 }\
            "
        ));
    }

    #[test]
    fn test_string_literals() {
        assert_snapshot!(strands(
            "{'foo' | 'bar\nbaz' | 'qux\\'quux' | 'quy\\\\quz' | 'xyz\\zy' }"
        ));
    }

    #[test]
    fn test_byte_literals() {
        assert_snapshot!(strands(
            "{b'foo' | b'bar\nbaz' | b'qux\\'quux' | b'quy\\\\quz' | b'xyz\\zy' }"
        ));
    }

    #[test]
    fn test_hex_literals() {
        assert_snapshot!(strands("{x'1234' | x'0d0e0f'}"));
    }

    #[test]
    fn test_vector_literals() {
        assert_snapshot!(strands(
            "{ vector[1u8, 2u8, 3u8, foo.bar, baz[42]] \
             | vector<u16>[]
             | vector<u32>
             | vector< u64 > [10, 11, 12,]
             | vector <u128,>[ 42u128 ]
             | vector<u256>[ ] }"
        ));
    }

    #[test]
    fn test_struct_positional_literals() {
        assert_snapshot!(strands(
            "{ 0x1::string::String(42, 'foo', vector[1, 2, 3]) \
             | 0x2::coin::Coin<0x2::sui::SUI> (true, 100u32,) }"
        ));
    }

    #[test]
    fn test_struct_named_literals() {
        assert_snapshot!(strands(
            "{ 0x1::string::String { length: 42, value: 'foo', data: vector[1, 2, 3], } \
             | 0x2::coin::Coin<0x2::sui::SUI> { is_locked: true, amount: 100u32 } }"
        ));
    }

    #[test]
    fn test_enum_positional_literals() {
        assert_snapshot!(strands(
            "{ 0x1::option::Option<u64>::Some#1(42,) \
             | 0x1::option::Option<u32>::1(43u32) \
             | 0x1::option::Option<u16>::0() }"
        ));
    }

    #[test]
    fn test_enum_named_literals() {
        assert_snapshot!(strands(
            "{ 0x1::option::Option<u64>::Some#1 { value: 42, } \
             | 0x1::option::Option<u32>::1 { value: 43u32 } \
             | 0x1::option::Option<u16>::None#0 {} }"
        ));
    }

    #[test]
    fn test_nested_datatype() {
        assert_snapshot!(strands(
            r#"{
                0x1::option::Option<0x2::coin::Coin<0x2::sui::SUI>>::Some#1(
                    0x2::coin::Coin<0x2::sui::SUI> {
                        balance: 100u64
                    }
                )
            }"#,
        ));
    }

    #[test]
    fn test_primitive_types() {
        assert_snapshot!(strands(
            "{ vector<address> \
             | vector<bool> \
             | vector<u8> \
             | vector<u16> \
             | vector<u32> \
             | vector<u64> \
             | vector<u128> \
             | vector<u256> }"
        ))
    }

    #[test]
    fn test_compound_types() {
        assert_snapshot!(strands(
            "{ vector<vector<u8>> \
             | vector<0x1::string::String> \
             | vector<0x2::coin::Coin< 0x2::sui::SUI >> \
             | vector<3::validator::Validator> }"
        ));
    }

    #[test]
    fn test_index_chain() {
        assert_snapshot!(strands(r#"{foo[bar][[baz]].qux[quy]}"#));
    }

    #[test]
    fn test_index_with_root() {
        assert_snapshot!(strands(r#"{true[[foo]][bar].baz}"#));
    }

    /**
     * Error Cases
     *
     * All the below cases are invalid syntax, and should return a parser error.
     */

    #[test]
    fn test_unbalanced_curlies() {
        assert_snapshot!(strands(r#"{foo"#));
    }

    #[test]
    fn test_missing_field_identifier() {
        assert_snapshot!(strands(r#"{foo..bar}"#));
    }

    #[test]
    fn test_unbalanced_index() {
        assert_snapshot!(strands(r#"{foo[bar}"#));
    }

    #[test]
    fn test_spaced_out_left_double_index() {
        assert_snapshot!(strands(r#"{foo[ [bar]]}"#));
    }

    #[test]
    fn test_spaced_out_right_double_index() {
        assert_snapshot!(strands(r#"{foo[[bar] ]}"#));
    }

    #[test]
    fn test_unbalanced_double_index() {
        assert_snapshot!(strands(r#"{foo[[bar}"#));
    }

    #[test]
    fn test_triple_index() {
        assert_snapshot!(strands(r#"{foo[[[bar]]]}"#));
    }

    #[test]
    fn test_unexpected_characters() {
        assert_snapshot!(strands(r#"anything goes {? % ! 🔥}"#));
    }

    #[test]
    fn test_trailing_alternate() {
        assert_snapshot!(strands(r#"{foo | bar | baz |}"#));
    }

    #[test]
    fn test_address_literal_whitespace() {
        assert_snapshot!(strands(r#"{@ 0x3}"#));
    }

    #[test]
    fn test_hex_address_overflow() {
        assert_snapshot!(strands(
            r#"{@0x12345678901234567890123456789012345678901234567890123456789012345}"#
        ));
    }

    #[test]
    fn test_dec_address_overflow() {
        assert_snapshot!(strands(
            r#"{@12345678901234567890123456789012345678901234567890123456789012345678901234567890}"#
        ));
    }

    #[test]
    fn test_address_literal_ident() {
        assert_snapshot!(strands(r#"{@foo}"#));
    }

    #[test]
    fn test_numeric_overflow_implicit() {
        assert_snapshot!(strands(r#"{ 678_901_234_567_890_123_456 }"#));
    }

    #[test]
    fn test_numeric_overflow_explicit() {
        assert_snapshot!(strands(r#"{ 0x90_1234_5678u32 }"#));
    }

    #[test]
    fn test_numeric_type_suffix_whitespace() {
        assert_snapshot!(strands(r#"{42 u64}"#));
    }

    #[test]
    fn test_trailing_string() {
        assert_snapshot!(strands(r#"{'foo"#));
    }

    #[test]
    fn test_byte_literal_whitespace() {
        assert_snapshot!(strands(r#"{b 'foo'}"#));
    }

    #[test]
    fn test_hex_literal_whitespace() {
        assert_snapshot!(strands(r#"{x '1234'}"#));
    }

    #[test]
    fn test_hex_literal_odd_length() {
        assert_snapshot!(strands(r#"{x'123'}"#));
    }

    #[test]
    fn test_hex_literal_invalid_char() {
        assert_snapshot!(strands(r#"{x'123g'}"#));
    }

    #[test]
    fn test_vector_literal_trailing() {
        assert_snapshot!(strands(r#"{vector[1, 2, 3"#));
    }

    #[test]
    fn test_vector_literal_arity() {
        assert_snapshot!(strands(r#"{vector<u64, u8>[1, 2, 3]}"#));
    }

    #[test]
    fn test_vector_literal_empty_no_type() {
        assert_snapshot!(strands(r#"{vector[]}"#));
    }

    #[test]
    fn test_vector_literal_empty_angles() {
        assert_snapshot!(strands(r#"{vector<>[1]}"#));
    }

    #[test]
    fn test_vector_keyword_only() {
        assert_snapshot!(strands(r#"{vector}"#));
    }

    #[test]
    fn test_vector_type_arity() {
        assert_snapshot!(strands(r#"{vector<vector<u64, u8>>}"#));
    }

    #[test]
    fn test_vector_type_no_type() {
        assert_snapshot!(strands(r#"{vector<vector>}"#));
    }

    #[test]
    fn test_vector_type_trailing() {
        assert_snapshot!(strands(r#"{vector<vector<u64"#));
    }

    #[test]
    fn test_vector_type_empty_angles() {
        assert_snapshot!(strands(r#"{vector<vector<>>}"#));
    }

    #[test]
    fn test_vector_literal_no_comma() {
        assert_snapshot!(strands(r#"{vector<u64>[1 2]}"#));
    }

    #[test]
    fn test_vector_element_error() {
        assert_snapshot!(strands(r#"{vector<u64>[1, 2, foo . 'bar']"#));
    }

    #[test]
    fn test_type_param_no_comma() {
        assert_snapshot!(strands(r#"{vector<0x2::table::Table<u64 u64>>}"#));
    }

    #[test]
    fn test_datatype_whitespace() {
        assert_snapshot!(strands(r#"{0x1 :: coin :: Coin<0x2::sui::SUI>}"#));
    }

    #[test]
    fn test_struct_literal_positional_trailing() {
        assert_snapshot!(strands(
            r#"{0x1::string::String(42, 'foo', vector[1, 2, 3]"#
        ));
    }

    #[test]
    fn test_struct_literal_named_trailing() {
        assert_snapshot!(strands(
            r#"{0x1::string::String { length: 42, value: 'foo', data: vector[1, 2, 3]"#,
        ));
    }

    #[test]
    fn test_enum_literal_positional_trailing() {
        assert_snapshot!(strands(r#"{0x1::option::Option<u64>::Some#1(42, 43, 44}"#));
    }

    #[test]
    fn test_enum_literal_named_trailing() {
        assert_snapshot!(strands(
            r#"{0x1::option::Option<u64>::Some#1 { value: 42, other: 43,"#,
        ));
    }

    #[test]
    fn test_struct_hybrid_positional_named() {
        assert_snapshot!(strands(
            r#"{0x1::string::String(length: 42, value: 'foo', data: vector[1, 2, 3])}"#
        ));
    }

    #[test]
    fn test_struct_hybrid_named_positional() {
        assert_snapshot!(strands(
            r#"{0x1::string::String { 42, 'foo', vector[1, 2, 3] }}"#
        ));
    }

    #[test]
    fn test_enum_missing_index() {
        assert_snapshot!(strands(r#"{0x1::option::Option<u64>::Some(42)}"#));
    }

    #[test]
    fn test_enum_whitespace() {
        assert_snapshot!(strands(r#"{0x1::option::Option<u64>::Some # 1 (42)}"#));
    }

    #[test]
    fn test_enum_variant_overflow() {
        assert_snapshot!(strands(r#"{0x1::option::Option<u64>::Some#70000(42)}"#));
    }
}
