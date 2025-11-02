// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::borrow::Cow;
use std::fmt;

use base64::engine::general_purpose::{
    GeneralPurpose, STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{StructTag, TypeTag},
    u256::U256,
};

use super::error::{Expected, ExpectedSet, FormatError, Match};
use super::peek::{Peekable2, Peekable2Ext};
use super::{
    lexer::{Lexeme as Lex, Lexer, Token as T},
    meter::Meter,
};

/// A single Display string template is a sequence of strands.
#[derive(PartialEq, Eq)]
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
#[derive(PartialEq, Eq)]
pub struct Expr<'s> {
    pub(crate) alternates: Vec<Chain<'s>>,
    pub(crate) transform: Option<Transform>,
}

/// Chains are a sequence of nested field accesses.
#[derive(PartialEq, Eq)]
pub struct Chain<'s> {
    /// An optional root expression. If not provided, the object being displayed is the root.
    pub(crate) root: Option<Literal<'s>>,

    /// A sequence of field accessors that go successively deeper into the object.
    pub(crate) accessors: Vec<Accessor<'s>>,
}

/// Different ways to nest deeply into an object.
#[derive(PartialEq, Eq)]
pub enum Accessor<'s> {
    /// Access a named field.
    Field(&'s IdentStr),

    /// Access a positional field.
    Positional(u8),

    /// Index into a vector, VecMap, or dynamic field.
    Index(Chain<'s>),

    /// Index into a dynamic field.
    DFIndex(Chain<'s>),

    /// Index into a dynamic object field.
    DOFIndex(Chain<'s>),
}

/// Literal forms are elements whose syntax determines their (outer) type.
#[derive(PartialEq, Eq)]
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
#[derive(PartialEq, Eq)]
pub struct Vector<'s> {
    /// Element type, optional for non-empty vectors.
    pub(crate) type_: Option<TypeTag>,
    pub(crate) elements: Vec<Chain<'s>>,
}

/// Contents of a struct literal.
#[derive(PartialEq, Eq)]
pub struct Struct<'s> {
    pub(crate) type_: StructTag,
    pub(crate) fields: Fields<'s>,
}

/// Contents of an enum literal.
#[derive(PartialEq, Eq)]
pub struct Enum<'s> {
    pub(crate) type_: StructTag,
    pub(crate) variant_name: Option<&'s str>,
    pub(crate) variant_index: u16,
    pub(crate) fields: Fields<'s>,
}

#[derive(PartialEq, Eq)]
pub enum Fields<'s> {
    Positional(Vec<Chain<'s>>),
    Named(Vec<(&'s str, Chain<'s>)>),
}

/// Ways to modify a value before displaying it.
#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub enum Transform {
    Base64(Base64Modifier),
    Bcs(Base64Modifier),
    Hex,
    #[default]
    Str,
    Timestamp,
    Url,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Base64Modifier(u8);

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
        $(
            $kind:ident($ws:pat, $($pat:path)|+, $off:pat, $slice:tt)
            $(@ $alias:ident)?
            $(if $cond:expr)? => $expr:expr
        ),+
        $(,)?
    ) => {{
        match $lexer.peek() {
            $(Some($($alias @)? &Lex($ws, $($pat)|+, $off, $slice)) $(if $cond)? => $expr,)+
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
        $(
            $kind:ident($ws:pat, $($pat:path)|+, $off:pat, $slice:tt)
            $(@ $alias:ident)?
            $(if $cond:expr)? => $expr:expr
        ),+
        $(,)?
    ) => {{
        match $lexer.peek() {
            $(Some($($alias @)? &Lex($ws, $($pat)|+, $off, $slice)) $(if $cond)? => Match::Found($expr),)+
            lexeme => Match::Tried(
                lexeme.map(|l| l.2),
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
///   expr     ::= '{' chain ('|' chain)* (':' xform)? '}'
///
///   chain    ::= (literal | IDENT) accessor*
///
///   accessor ::= '.' IDENT
///              | '.' NUM_DEC
///              | '[' chain ']'
///              | '->' '[' chain ']'
///              | '=>' '[' chain ']'
///
///   literal  ::= address | bool | number | string | vector | struct | enum
///
///   address  ::= '@' (NUM_DEC | NUM_HEX)
///
///   bool     ::= 'true' | 'false'
///
///   number   ::= (NUM_DEC | NUM_HEX) numeric
///
///   string   ::= ('b' | 'x')? STRING
///
///   vector   ::= 'vector'  '<' type ','? '>' ('[' ']')?
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
///   type     ::= 'address' | 'bool' | | 'vector' '<' type '>' |  numeric | datatype
///
///   datatype ::= NUM_HEX '::' IDENT ('<' type (',' type)* ','? '>')?
///
///   numeric  ::= 'u8' | 'u16' | 'u32' | 'u64' | 'u128' | 'u256'
///
///   xform    ::= 'str'
///              | 'hex'
///              | 'base64' xmod?
///              | 'bcs'
///              | 'timestamp'
///              | 'url'
///
///  xmod      ::= '(' b64mod (',' b64mod)* ','? )'
///
///  b64mod    ::= 'url' | 'nopad'
///
impl<'s> Parser<'s> {
    /// Construct a new parser, consuming input from the `src` string.
    pub(crate) fn new(src: &'s str) -> Self {
        Self {
            lexer: Lexer::new(src).peekable2(),
        }
    }

    /// Entrypoint into the parser, parsing the root non-terminal -- `format`.
    pub(crate) fn run<'b>(
        src: &'s str,
        meter: &mut Meter<'b>,
    ) -> Result<Vec<Strand<'s>>, FormatError> {
        Self::new(src).parse_format(meter)
    }

    fn parse_format<'b>(mut self, meter: &mut Meter<'b>) -> Result<Vec<Strand<'s>>, FormatError> {
        let mut strands = vec![];
        while self.lexer.peek().is_some() {
            strands.push(self.parse_strand(meter)?);
        }

        Ok(strands)
    }

    fn parse_strand<'b>(&mut self, meter: &mut Meter<'b>) -> Result<Strand<'s>, FormatError> {
        Ok(match_token! { self.lexer;
            Tok(_, T::Text | T::LLBrace | T::RRBrace, _, _) => Strand::Text(self.parse_text(meter)?),
            Tok(_, T::LBrace, _, _) => Strand::Expr(self.parse_expr(meter)?),
        })
    }

    fn parse_text<'b>(&mut self, meter: &mut Meter<'b>) -> Result<Cow<'s, str>, FormatError> {
        let mut text = self.parse_part()?;
        while let Some(Lex(_, T::Text | T::LLBrace | T::RRBrace, _, _)) = self.lexer.peek() {
            text += self.parse_part()?;
        }

        meter.alloc()?;
        Ok(text)
    }

    fn parse_part(&mut self) -> Result<Cow<'s, str>, FormatError> {
        Ok(match_token! { self.lexer;
            Tok(_, T::Text | T::LLBrace | T::RRBrace, _, slice) => {
                self.lexer.next();
                Cow::Borrowed(slice)
            }
        })
    }

    fn parse_expr<'b>(&mut self, meter: &mut Meter<'b>) -> Result<Expr<'s>, FormatError> {
        match_token! { self.lexer; Tok(_, T::LBrace, _, _) => self.lexer.next() };
        let mut alternates = vec![self.parse_chain(meter)?];
        let mut transform = None;

        loop {
            match_token! { self.lexer;
                Tok(_, T::RBrace, _, _) => {
                    self.lexer.next();
                    break;
                },

                Tok(_, T::Colon, _, _) => {
                    self.lexer.next();
                    transform = Some(self.parse_xform()?);
                    match_token! { self.lexer; Tok(_, T::RBrace, _, _) => {
                        self.lexer.next()
                    }};
                    break;
                },

                Tok(_, T::Pipe, _, _) => {
                    self.lexer.next();
                    alternates.push(self.parse_chain(meter)?);
                }
            }
        }

        meter.alloc()?;
        Ok(Expr {
            alternates,
            transform,
        })
    }

    fn parse_chain<'b>(&mut self, meter: &mut Meter<'b>) -> Result<Chain<'s>, FormatError> {
        let meter = &mut meter.nest()?;
        let mut accessors = vec![];

        // If there is no root literal, the chain must start with an identifier, representing a
        // field on the object being displayed.
        let root = match self.try_parse_literal(meter)? {
            Match::Found(literal) => Some(literal),
            Match::Tried(_, tried) => {
                accessors.push(match_token! { self.lexer, tried;
                    Tok(_, T::Ident, _, ident) @ lex => {
                        let ident = IdentStr::new(ident).map_err(|_| FormatError::InvalidIdentifier(lex.detach()))?;
                        self.lexer.next();
                        meter.alloc()?;
                        Accessor::Field(ident)
                    }},
                );
                None
            }
        };

        while let Match::Found(accessor) = self.try_parse_accessor(meter)? {
            accessors.push(accessor);
        }

        meter.alloc()?;
        Ok(Chain { root, accessors })
    }

    fn try_parse_literal<'b>(
        &mut self,
        meter: &mut Meter<'b>,
    ) -> Result<Match<Literal<'s>>, FormatError> {
        Ok(match_token_opt! { self.lexer;
            Tok(_, T::At, _, _) => {
                self.lexer.next();
                meter.alloc()?;
                Literal::Address(self.parse_address()?)
            },

            Lit(_, T::Ident, _, "true") => {
                self.lexer.next();
                meter.alloc()?;
                Literal::Bool(true)
            },

            Lit(_, T::Ident, _, "false") => {
                self.lexer.next();
                meter.alloc()?;
                Literal::Bool(false)
            },

            Tok(_, T::NumDec | T::NumHex, _, _)
            if self.lexer.peek2().is_some_and(|Lex(_, t, _, _)| *t == T::CColon) => {
                self.parse_data(meter)?
            },

            Tok(_, T::NumDec, _, dec) => {
                self.lexer.next();
                self.parse_numeric_suffix(dec, 10, meter)?
            },

            Tok(_, T::NumHex, _, hex) => {
                self.lexer.next();
                self.parse_numeric_suffix(hex, 16, meter)?
            },

            Tok(_, T::String, _, slice) => {
                self.lexer.next();
                meter.alloc()?;
                Literal::String(read_string_literal(slice))
            },

            Lit(_, T::Ident, _, "b")
            if self.lexer.peek2().is_some_and(|Lex(ws, t, _, _)| !ws && *t == T::String) => {
                self.lexer.next();

                // SAFETY: Match guard peeks ahead to this token.
                let Lex(_, _, _, slice) = self.lexer.next().unwrap();
                let output = read_string_literal(slice);

                meter.alloc()?;
                Literal::ByteArray(output.into_owned().into_bytes())
            },

            Lit(_, T::Ident, _, "x")
            if self.lexer.peek2().is_some_and(|Lex(ws, t, _, _)| !ws && *t == T::String) => {
                self.lexer.next();

                // SAFETY: Match guard peeks ahead to this token.
                let lex @ Lex(_, _, _, slice) = self.lexer.next().unwrap();
                let output = read_hex_literal(&lex, slice)?;

                meter.alloc()?;
                Literal::ByteArray(output)
            },

            Tok(_, T::Ident, offset, "vector") => {
                self.lexer.next();

                let mut type_params = self.parse_type_params(meter)?;
                let type_ = match type_params.len() {
                    // SAFETY: Bounds check on `type_params.len()` guarantees safety.
                    1 => Some(type_params.pop().unwrap()),
                    0 => None,
                    arity => return Err(FormatError::VectorArity { offset, arity }),
                };

                let elements = self.parse_array_elements(meter)?;

                // If the vector is empty, the type parameter becomes mandatory.
                if elements.is_empty() && type_.is_none() {
                    return Err(FormatError::VectorArity {
                        offset,
                        arity: 0,
                    });
                }

                meter.alloc()?;
                Literal::Vector(Box::new(Vector { type_, elements }))
            },
        })
    }

    fn try_parse_accessor<'b>(
        &mut self,
        meter: &mut Meter<'b>,
    ) -> Result<Match<Accessor<'s>>, FormatError> {
        Ok(match_token_opt! { self.lexer;
            Tok(_, T::Dot, _, _) => {
                self.lexer.next();
                match_token! { self.lexer;
                    Tok(_, T::Ident, _, ident) @ lex => {
                        let ident = IdentStr::new(ident)
                            .map_err(|_| FormatError::InvalidIdentifier(lex.detach()))?;
                        self.lexer.next();
                        meter.alloc()?;
                        Accessor::Field(ident)
                    },

                    Tok(_, T::NumDec, _, n) => {
                        self.lexer.next();
                        let index = read_u8(n, 10, "positional field index")?;

                        meter.alloc()?;
                        Accessor::Positional(index)
                    }
                }
            },

            Tok(_, T::Arrow, _, _) => {
                self.lexer.next();

                match_token! { self.lexer; Tok(_, T::LBracket, _, _) => self.lexer.next() };
                let chain = self.parse_chain(meter)?;
                match_token! { self.lexer; Tok(_, T::RBracket, _, _) => self.lexer.next() };

                meter.load()?;
                meter.alloc()?;
                Accessor::DFIndex(chain)
            },

            Tok(_, T::AArrow, _, _) => {
                self.lexer.next();

                match_token! { self.lexer; Tok(_, T::LBracket, _, _) => self.lexer.next() };
                let chain = self.parse_chain(meter)?;
                match_token! { self.lexer; Tok(_, T::RBracket, _, _) => self.lexer.next() };

                // Dynamic Object Fields require two successive loads.
                meter.load()?;
                meter.load()?;
                meter.alloc()?;
                Accessor::DOFIndex(chain)
            },

            Tok(_, T::LBracket, _, _) => {
                self.lexer.next();

                let chain = self.parse_chain(meter)?;
                match_token! { self.lexer; Tok(_, T::RBracket, _, _) => self.lexer.next() };

                meter.alloc()?;
                Accessor::Index(chain)
            },
        })
    }

    fn parse_array_elements<'b>(
        &mut self,
        meter: &mut Meter<'b>,
    ) -> Result<Vec<Chain<'s>>, FormatError> {
        let mut elements = vec![];

        if match_token_opt! { self.lexer; Tok(_, T::LBracket, _, _) => { self.lexer.next(); } }
            .is_not_found()
        {
            return Ok(elements);
        }

        let terminated = match_token_opt! { self.lexer;
            Tok(_, T::RBracket, _, _) => { self.lexer.next(); }
        };

        let (offset, terminated) = match terminated {
            Match::Tried(offset, terminated) => (offset, terminated),
            Match::Found(_) => return Ok(elements),
        };

        loop {
            elements.push(
                self.parse_chain(meter)
                    .map_err(|e| e.also_tried(offset, terminated.clone()))?,
            );

            let delimited = match_token_opt! { self.lexer;
                Tok(_, T::Comma, _, _) => { self.lexer.next(); }
            };

            let terminated = match_token_opt! { self.lexer;
                Tok(_, T::RBracket, _, _) => { self.lexer.next(); }
            };

            match (delimited, terminated) {
                (_, Match::Found(_)) => break,
                (Match::Found(_), _) => continue,
                (Match::Tried(_, delimited), Match::Tried(_, terminated)) => {
                    return Err(delimited.union(terminated).into_error(self.lexer.peek()));
                }
            }
        }

        Ok(elements)
    }

    fn parse_data<'b>(&mut self, meter: &mut Meter<'b>) -> Result<Literal<'s>, FormatError> {
        let type_ = self.parse_datatype(meter)?;

        let enum_ = match_token_opt! { self.lexer;
            Tok(_, T::CColon, _, _) => { self.lexer.next(); }
        };

        if let Match::Tried(offset, enum_) = enum_ {
            meter.alloc()?;
            return Ok(Literal::Struct(Box::new(Struct {
                type_,
                fields: self
                    .parse_fields(meter)
                    .map_err(|e| e.also_tried(offset, enum_))?,
            })));
        }

        Ok(match_token! { self.lexer;
            Tok(_, T::Ident, _, variant_name) => {
                self.lexer.next();

                match_token! { self.lexer; Tok(_, T::Pound, _, _) => self.lexer.next() };
                let variant_index = match_token! { self.lexer; Tok(_, T::NumDec, _, index) => {
                    self.lexer.next();
                    read_u16(index, 10, "enum variant index")?
                }};

                meter.alloc()?;
                Literal::Enum(Box::new(Enum {
                    type_,
                    variant_name: Some(variant_name),
                    variant_index,
                    fields: self.parse_fields(meter)?,
                }))
            },

            Tok(_, T::NumDec, _, index) => {
                self.lexer.next();

                let variant_index = read_u16(index, 10, "enum variant index")?;

                meter.alloc()?;
                Literal::Enum(Box::new(Enum {
                    type_,
                    variant_name: None,
                    variant_index,
                    fields: self.parse_fields(meter)?,
                }))
            }
        })
    }

    fn parse_fields<'b>(&mut self, meter: &mut Meter<'b>) -> Result<Fields<'s>, FormatError> {
        let is_named = match_token! { self.lexer;
            Tok(_, T::LParen, _, _) => { self.lexer.next(); false },
            Tok(_, T::LBrace, _, _) => { self.lexer.next(); true }
        };

        Ok(if is_named {
            let mut fields = vec![];

            let terminated = match_token_opt! { self.lexer;
                Tok(_, T::RBrace, _, _) => { self.lexer.next(); }
            };

            let terminated = match terminated {
                Match::Tried(_, terminated) => terminated,
                Match::Found(_) => return Ok(Fields::Named(fields)),
            };

            loop {
                let name = match_token! { self.lexer, terminated;
                    Tok(_, T::Ident, _, n) => { self.lexer.next(); n }
                };

                match_token! { self.lexer; Tok(_, T::Colon, _, _) => self.lexer.next() };

                let value = self.parse_chain(meter)?;
                fields.push((name, value));

                let delimited = match_token_opt! { self.lexer;
                    Tok(_, T::Comma, _, _) => { self.lexer.next(); }
                };

                let terminated = match_token_opt! { self.lexer;
                    Tok(_, T::RBrace, _, _) => { self.lexer.next(); }
                };

                match (delimited, terminated) {
                    (_, Match::Found(_)) => break,
                    (Match::Found(_), _) => continue,
                    (Match::Tried(_, delimited), Match::Tried(_, terminated)) => {
                        return Err(delimited.union(terminated).into_error(self.lexer.peek()));
                    }
                }
            }

            Fields::Named(fields)
        } else {
            let mut fields = vec![];

            let terminated = match_token_opt! { self.lexer;
                Tok(_, T::RParen, _, _) => { self.lexer.next(); }
            };

            let (offset, terminated) = match terminated {
                Match::Tried(offset, terminated) => (offset, terminated),
                Match::Found(_) => return Ok(Fields::Positional(fields)),
            };

            loop {
                fields.push(
                    self.parse_chain(meter)
                        .map_err(|e| e.also_tried(offset, terminated.clone()))?,
                );

                let delimited = match_token_opt! { self.lexer;
                    Tok(_, T::Comma, _, _) => { self.lexer.next(); }
                };

                let terminated = match_token_opt! { self.lexer;
                    Tok(_, T::RParen, _, _) => { self.lexer.next(); }
                };

                match (delimited, terminated) {
                    (_, Match::Found(_)) => break,
                    (Match::Found(_), _) => continue,
                    (Match::Tried(_, delimited), Match::Tried(_, terminated)) => {
                        return Err(delimited.union(terminated).into_error(self.lexer.peek()));
                    }
                }
            }

            Fields::Positional(fields)
        })
    }

    fn parse_type(&mut self, meter: &mut Meter<'_>) -> Result<TypeTag, FormatError> {
        let meter = &mut meter.nest()?;
        Ok(match_token! { self.lexer;
            Lit(_, T::Ident, _, "address") => {
                self.lexer.next();
                meter.alloc()?;
                TypeTag::Address
            },

            Lit(_, T::Ident, _, "bool") => {
                self.lexer.next();
                meter.alloc()?;
                TypeTag::Bool
            },

            Lit(_, T::Ident, _, "u8") => {
                self.lexer.next();
                meter.alloc()?;
                TypeTag::U8
            },

            Lit(_, T::Ident, _, "u16") => {
                self.lexer.next();
                meter.alloc()?;
                TypeTag::U16
            },

            Lit(_, T::Ident, _, "u32") => {
                self.lexer.next();
                meter.alloc()?;
                TypeTag::U32
            },

            Lit(_, T::Ident, _, "u64") => {
                self.lexer.next();
                meter.alloc()?;
                TypeTag::U64
            },

            Lit(_, T::Ident, _, "u128") => {
                self.lexer.next();
                meter.alloc()?;
                TypeTag::U128
            },

            Lit(_, T::Ident, _, "u256") => {
                self.lexer.next();
                meter.alloc()?;
                TypeTag::U256
            },

            Lit(_, T::Ident, offset, "vector") => {
                self.lexer.next();
                let mut type_params = self.parse_type_params(meter)?;
                match type_params.len() {
                    1 => {
                        // SAFETY: Bounds check on `type_params.len()` guarantees safety.
                        let inner = type_params.pop().unwrap();

                        meter.alloc()?;
                        TypeTag::Vector(Box::new(inner))
                    }

                    arity => return Err(FormatError::VectorArity { offset, arity }),
                }
            },

            Tok(_, T::NumDec | T::NumHex, _, _) => {
                TypeTag::Struct(Box::new(self.parse_datatype(meter)?))
            }
        })
    }

    fn parse_datatype(&mut self, meter: &mut Meter<'_>) -> Result<StructTag, FormatError> {
        let address = self.parse_address()?;

        match_token! { self.lexer; Tok(_, T::CColon, _, _) => self.lexer.next() };
        let module = self.parse_identifier()?;

        match_token! { self.lexer; Tok(_, T::CColon, _, _) => self.lexer.next() };
        let name = self.parse_identifier()?;

        let type_params = self.parse_type_params(meter)?;

        meter.alloc()?;
        Ok(StructTag {
            address,
            module,
            name,
            type_params,
        })
    }

    fn parse_type_params(&mut self, meter: &mut Meter<'_>) -> Result<Vec<TypeTag>, FormatError> {
        let mut type_params = vec![];
        if match_token_opt! { self.lexer; Tok(_, T::LAngle, _, _) => { self.lexer.next(); } }
            .is_not_found()
        {
            return Ok(type_params);
        }

        loop {
            type_params.push(self.parse_type(meter)?);

            let delimited = match_token_opt! { self.lexer;
                Tok(_, T::Comma, _, _) => { self.lexer.next(); }
            };

            let terminated = match_token_opt! { self.lexer;
                Tok(_, T::RAngle, _, _) => { self.lexer.next(); }
            };

            match (delimited, terminated) {
                (_, Match::Found(_)) => break,
                (Match::Found(_), _) => continue,
                (Match::Tried(_, delimited), Match::Tried(_, terminated)) => {
                    return Err(delimited.union(terminated).into_error(self.lexer.peek()));
                }
            }
        }

        Ok(type_params)
    }

    fn parse_address(&mut self) -> Result<AccountAddress, FormatError> {
        let addr = match_token! { self.lexer;
            Tok(_, T::NumHex, _, slice) => {
                self.lexer.next();
                read_u256(slice, 16, "'address'")?
            },

            Tok(_, T::NumDec, _, slice) => {
                self.lexer.next();
                read_u256(slice, 10, "'address'")?
            },
        };

        Ok(AccountAddress::from(addr.to_be_bytes()))
    }

    fn parse_identifier(&mut self) -> Result<Identifier, FormatError> {
        match_token! { self.lexer;
            Tok(_, T::Ident, _, ident) @ lex => {
                let ident = Identifier::new(ident).map_err(|_| FormatError::InvalidIdentifier(lex.detach()));
                self.lexer.next();
                ident
            },
        }
    }

    fn parse_numeric_suffix<'b>(
        &mut self,
        num: &'s str,
        radix: u32,
        meter: &mut Meter<'b>,
    ) -> Result<Literal<'s>, FormatError> {
        Ok(match_token! { self.lexer;
            Lit(false, T::Ident, _, "u8") => {
                self.lexer.next();
                meter.alloc()?;
                Literal::U8(read_u8(num, radix, "'u8'")?)
            },

            Lit(false, T::Ident, _, "u16") => {
                self.lexer.next();
                meter.alloc()?;
                Literal::U16(read_u16(num, radix, "'u16'")?)
            },

            Lit(false, T::Ident, _, "u32") => {
                self.lexer.next();
                meter.alloc()?;
                Literal::U32(read_u32(num, radix, "'u32'")?)
            },

            Lit(false, T::Ident, _, "u64") => {
                self.lexer.next();
                meter.alloc()?;
                Literal::U64(read_u64(num, radix, "'u64'")?)
            },

            Lit(false, T::Ident, _, "u128") => {
                self.lexer.next();
                meter.alloc()?;
                Literal::U128(read_u128(num, radix, "'u128'")?)
            },

            Lit(false, T::Ident, _, "u256") => {
                self.lexer.next();
                meter.alloc()?;
                Literal::U256(read_u256(num, radix, "'u256'")?)
            },
        })
    }

    fn parse_xform(&mut self) -> Result<Transform, FormatError> {
        Ok(match_token! { self.lexer;
            Lit(_, T::Ident, _, "base64") => {
                self.lexer.next();
                Transform::Base64(self.parse_xmod()?)
            },

            Lit(_, T::Ident, _, "bcs") => {
                self.lexer.next();
                Transform::Bcs(self.parse_xmod()?)
            },

            Lit(_, T::Ident, _, "hex") => {
                self.lexer.next();
                Transform::Hex
            },

            Lit(_, T::Ident, _, "str") => {
                self.lexer.next();
                Transform::Str
            },

            Lit(_, T::Ident, _, "ts") => {
                self.lexer.next();
                Transform::Timestamp
            },

            Lit(_, T::Ident, _, "url") => {
                self.lexer.next();
                Transform::Url
            },
        })
    }

    fn parse_xmod(&mut self) -> Result<Base64Modifier, FormatError> {
        let mut xmod = Base64Modifier::EMPTY;
        if match_token_opt! { self.lexer; Tok(_, T::LParen, _, _) => { self.lexer.next(); } }
            .is_not_found()
        {
            return Ok(xmod);
        }

        loop {
            xmod = xmod.union(match_token! { self.lexer;
                Lit(_, T::Ident, _, "url") => { self.lexer.next(); Base64Modifier::URL },
                Lit(_, T::Ident, _, "nopad") => { self.lexer.next(); Base64Modifier::NOPAD },
            });

            let delimited = match_token_opt! { self.lexer;
                Tok(_, T::Comma, _, _) => { self.lexer.next(); }
            };

            let terminated = match_token_opt! { self.lexer;
                Tok(_, T::RParen, _, _) => { self.lexer.next(); }
            };

            match (delimited, terminated) {
                (_, Match::Found(_)) => break,
                (Match::Found(_), _) => continue,
                (Match::Tried(_, delimited), Match::Tried(_, terminated)) => {
                    return Err(delimited.union(terminated).into_error(self.lexer.peek()));
                }
            }
        }

        Ok(xmod)
    }
}

impl Base64Modifier {
    /// Use a standard Base64 encoding.
    const EMPTY: Self = Self(0);

    /// Use the URL-safe character set.
    const URL: Self = Self(1 << 1);

    /// Don't add padding characters.
    const NOPAD: Self = Self(1 << 2);

    pub fn standard(&self) -> bool {
        self.0 == Self::EMPTY.0
    }

    pub fn url(&self) -> bool {
        self.0 & Self::URL.0 > 0
    }

    pub fn nopad(&self) -> bool {
        self.0 & Self::NOPAD.0 > 0
    }

    /// The Base64 encoding engine for this set of modifiers
    pub fn engine(&self) -> &'static GeneralPurpose {
        if self.url() && self.nopad() {
            &URL_SAFE_NO_PAD
        } else if self.url() {
            &URL_SAFE
        } else if self.nopad() {
            &STANDARD_NO_PAD
        } else {
            &STANDARD
        }
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl fmt::Debug for Strand<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Strand::Text(text) => write!(f, "{text:?}"),
            Strand::Expr(expr) if f.alternate() => write!(f, "{expr:#?}"),
            Strand::Expr(expr) => write!(f, "{expr:?}"),
        }
    }
}

impl fmt::Debug for Expr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut prefix = "{ ";
        for alternate in &self.alternates {
            write!(f, "{prefix}")?;
            if f.alternate() {
                let alternate = format!("{alternate:#?}").replace('\n', "\n  ");
                write!(f, "{alternate}")?;
                prefix = "\n| ";
            } else {
                write!(f, "{alternate:?}")?;
                prefix = " | ";
            }
        }

        if f.alternate() {
            writeln!(f)?;
        }

        if let Some(transform) = self.transform {
            write!(f, ": {transform:?}")?;
        }

        write!(f, "}}")
    }
}

impl fmt::Debug for Chain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut accessors = self.accessors.iter();
        if let Some(root) = &self.root {
            root.fmt(f)?;
        } else if let Some(Accessor::Field(name)) = accessors.next() {
            // If there is no root, the first accessor is guaranteed to exist, and it must be a
            // field accessor.
            write!(f, "{name}")?;
        }

        for accessor in accessors {
            use Accessor as A;
            match accessor {
                A::Field(name) => write!(f, ".{name}")?,
                A::Positional(index) => write!(f, ".{index}")?,
                A::Index(chain) => write!(f, "[{chain:?}]")?,
                A::DFIndex(chain) => write!(f, "->[{chain:?}]")?,
                A::DOFIndex(chain) => write!(f, "=>[{chain:?}]")?,
            }
        }

        Ok(())
    }
}

impl fmt::Debug for Literal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Address(addr) => write!(f, "@0x{addr}"),
            Literal::Bool(b) => write!(f, "{b}"),
            Literal::U8(n) => write!(f, "{n}u8"),
            Literal::U16(n) => write!(f, "{n}u16"),
            Literal::U32(n) => write!(f, "{n}u32"),
            Literal::U64(n) => write!(f, "{n}u64"),
            Literal::U128(n) => write!(f, "{n}u128"),
            Literal::U256(n) => write!(f, "{n}u256"),
            Literal::String(s) => write!(f, "{s:?}"),
            Literal::Vector(v) => v.fmt(f),
            Literal::Struct(s) => s.fmt(f),
            Literal::Enum(e) => e.fmt(f),

            Literal::ByteArray(bytes) => {
                write!(f, "x'")?;
                for byte in bytes {
                    write!(f, "{byte:02x}")?;
                }
                write!(f, "'")
            }
        }
    }
}

impl fmt::Debug for Vector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "vector")?;
        if let Some(type_) = &self.type_ {
            write!(f, "<{}> ", type_.to_canonical_display(true))?;
        }

        let mut builder = f.debug_list();
        for element in &self.elements {
            builder.entry(element);
        }

        builder.finish()
    }
}

impl fmt::Debug for Struct<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.fields {
            Fields::Named(fields) => {
                let mut builder = f.debug_struct(&self.type_.to_canonical_string(true));
                for (name, chain) in fields {
                    builder.field(name, &chain);
                }

                builder.finish()
            }

            Fields::Positional(fields) => {
                let mut builder = f.debug_tuple(&self.type_.to_canonical_string(true));
                for chain in fields {
                    builder.field(&chain);
                }

                builder.finish()
            }
        }
    }
}

impl fmt::Debug for Enum<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::", self.type_.to_canonical_display(true))?;
        if let Some(variant_name) = self.variant_name {
            write!(f, "{variant_name}#")?;
        }

        match &self.fields {
            Fields::Named(fields) => {
                let mut builder = f.debug_struct(&self.variant_index.to_string());
                for (name, chain) in fields {
                    builder.field(name, &chain);
                }

                builder.finish()
            }

            Fields::Positional(fields) => {
                let mut builder = f.debug_tuple(&self.variant_index.to_string());
                for chain in fields {
                    builder.field(&chain);
                }

                builder.finish()
            }
        }
    }
}

impl fmt::Debug for Transform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Transform::Base64(xmod) => write!(f, "base64{xmod:?}"),
            Transform::Bcs(xmod) => write!(f, "bcs{xmod:?}"),
            Transform::Hex => write!(f, "hex"),
            Transform::Str => write!(f, "str"),
            Transform::Timestamp => write!(f, "ts"),
            Transform::Url => write!(f, "url"),
        }
    }
}

impl fmt::Debug for Base64Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut prefix = "(";

        if self.url() {
            f.write_str(prefix)?;
            f.write_str("url")?;
            prefix = ", ";
        }

        if self.nopad() {
            f.write_str(prefix)?;
            f.write_str("nopad")?;
        }

        if prefix != "(" {
            f.write_str(")")?;
        }

        Ok(())
    }
}

fn read_u8(slice: &str, radix: u32, what: &'static str) -> Result<u8, FormatError> {
    u8::from_str_radix(&slice.replace('_', ""), radix).map_err(|err| FormatError::InvalidNumber {
        what,
        err: err.to_string(),
    })
}

fn read_u16(slice: &str, radix: u32, what: &'static str) -> Result<u16, FormatError> {
    u16::from_str_radix(&slice.replace('_', ""), radix).map_err(|err| FormatError::InvalidNumber {
        what,
        err: err.to_string(),
    })
}

fn read_u32(slice: &str, radix: u32, what: &'static str) -> Result<u32, FormatError> {
    u32::from_str_radix(&slice.replace('_', ""), radix).map_err(|err| FormatError::InvalidNumber {
        what,
        err: err.to_string(),
    })
}

fn read_u64(slice: &str, radix: u32, what: &'static str) -> Result<u64, FormatError> {
    u64::from_str_radix(&slice.replace('_', ""), radix).map_err(|err| FormatError::InvalidNumber {
        what,
        err: err.to_string(),
    })
}

fn read_u128(slice: &str, radix: u32, what: &'static str) -> Result<u128, FormatError> {
    u128::from_str_radix(&slice.replace('_', ""), radix).map_err(|err| FormatError::InvalidNumber {
        what,
        err: err.to_string(),
    })
}

fn read_u256(slice: &str, radix: u32, what: &'static str) -> Result<U256, FormatError> {
    U256::from_str_radix(&slice.replace('_', ""), radix).map_err(|err| FormatError::InvalidNumber {
        what,
        err: err.to_string(),
    })
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

fn read_hex_literal(lexeme: &Lex<'_>, slice: &str) -> Result<Vec<u8>, FormatError> {
    if !slice.len().is_multiple_of(2) {
        return Err(FormatError::OddHexLiteral(lexeme.detach()));
    }

    let mut output = Vec::with_capacity(slice.len() / 2);
    for i in (0..slice.len()).step_by(2) {
        let byte = u8::from_str_radix(&slice[i..i + 2], 16)
            .map_err(|_| FormatError::InvalidHexCharacter(lexeme.detach()))?;
        output.push(byte);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use insta::assert_snapshot;
    use move_core_types::ident_str;

    use crate::v2::meter::Limits;

    use super::{Accessor as A, Chain as C, Expr as E, Literal as L, Parser, Strand as S, *};

    fn strands(src: &str) -> String {
        let limits = Limits::default();
        let mut budget = limits.budget();
        let mut meter = Meter::new(limits.max_depth, &mut budget);

        let strands = match Parser::run(src, &mut meter) {
            Ok(strands) => strands,
            Err(e) => return format!("Error: {e}"),
        };

        let mut output = String::new();
        for strand in strands {
            output += &format!("{strand:#?}\n");
        }

        output
    }

    fn nodes_and_loads(src: &str) -> (usize, usize, Vec<Strand<'_>>) {
        let limits = Limits {
            max_depth: usize::MAX,
            max_nodes: usize::MAX,
            max_loads: usize::MAX,
        };

        let mut budget = limits.budget();
        let mut meter = Meter::new(limits.max_depth, &mut budget);

        let strands = Parser::new(src).parse_format(&mut meter).unwrap();
        (
            usize::MAX - budget.nodes,
            usize::MAX - budget.loads,
            strands,
        )
    }

    fn parse_with_depth(depth: usize, src: &str) -> Result<Vec<Strand<'_>>, FormatError> {
        let limits = Limits {
            max_depth: depth,
            max_nodes: usize::MAX,
            max_loads: usize::MAX,
        };

        let mut budget = limits.budget();
        let mut meter = Meter::new(limits.max_depth, &mut budget);

        Parser::new(src).parse_format(&mut meter)
    }

    #[test]
    fn test_metering_text() {
        let (nodes, loads, strands) = nodes_and_loads("foo bar");
        assert_eq!(nodes, 1);
        assert_eq!(loads, 0);
        assert_eq!(strands, vec![S::Text("foo bar".into())]);
    }

    #[test]
    fn test_metering_text_with_escapes() {
        let (nodes, loads, strands) = nodes_and_loads("foo {{bar}}");
        assert_eq!(nodes, 1);
        assert_eq!(loads, 0);
        assert_eq!(strands, vec![S::Text("foo {bar}".into())]);
    }

    #[test]
    fn test_metering_expression() {
        let (nodes, loads, strands) = nodes_and_loads("{foo}");
        assert_eq!(nodes, 3);
        assert_eq!(loads, 0);
        assert_eq!(
            strands,
            vec![S::Expr(E {
                alternates: vec![C {
                    root: None,
                    accessors: vec![A::Field(ident_str!("foo"))],
                }],
                transform: None,
            })]
        );
    }

    #[test]
    fn test_metering_expression_with_transform() {
        let (nodes, loads, strands) = nodes_and_loads("{foo:str}");
        assert_eq!(nodes, 3);
        assert_eq!(loads, 0);
        assert_eq!(
            strands,
            vec![S::Expr(E {
                alternates: vec![C {
                    root: None,
                    accessors: vec![A::Field(ident_str!("foo"))],
                }],
                transform: Some(Transform::Str),
            })]
        );
    }

    #[test]
    fn test_metering_field_access() {
        let (nodes, loads, strands) = nodes_and_loads("{foo.bar}");
        assert_eq!(nodes, 4);
        assert_eq!(loads, 0);
        assert_eq!(
            strands,
            vec![S::Expr(E {
                alternates: vec![C {
                    root: None,
                    accessors: vec![A::Field(ident_str!("foo")), A::Field(ident_str!("bar"))],
                }],
                transform: None,
            })]
        );
    }

    #[test]
    fn test_metering_text_and_expr() {
        let (nodes, loads, strands) = nodes_and_loads("foo {bar} baz");
        assert_eq!(nodes, 5);
        assert_eq!(loads, 0);
        assert_eq!(
            strands,
            vec![
                S::Text("foo ".into()),
                S::Expr(E {
                    alternates: vec![C {
                        root: None,
                        accessors: vec![A::Field(ident_str!("bar"))],
                    }],
                    transform: None,
                }),
                S::Text(" baz".into()),
            ]
        );
    }

    #[test]
    fn test_metering_alternates() {
        let (nodes, loads, strands) = nodes_and_loads("{foo | bar | baz}");
        assert_eq!(nodes, 7);
        assert_eq!(loads, 0);
        assert_eq!(
            strands,
            vec![S::Expr(E {
                alternates: vec![
                    C {
                        root: None,
                        accessors: vec![A::Field(ident_str!("foo"))],
                    },
                    C {
                        root: None,
                        accessors: vec![A::Field(ident_str!("bar"))],
                    },
                    C {
                        root: None,
                        accessors: vec![A::Field(ident_str!("baz"))],
                    },
                ],
                transform: None,
            })]
        );
    }

    #[test]
    fn test_metering_indexed_access() {
        let (nodes, loads, strands) = nodes_and_loads("{foo[bar]->[baz]}");
        assert_eq!(nodes, 9);
        assert_eq!(loads, 1);
        assert_eq!(
            strands,
            vec![S::Expr(E {
                alternates: vec![C {
                    root: None,
                    accessors: vec![
                        A::Field(ident_str!("foo")),
                        A::Index(C {
                            root: None,
                            accessors: vec![A::Field(ident_str!("bar"))],
                        }),
                        A::DFIndex(C {
                            root: None,
                            accessors: vec![A::Field(ident_str!("baz"))],
                        }),
                    ],
                }],
                transform: None,
            })]
        );
    }

    #[test]
    fn test_metering_nested_loads() {
        let (nodes, loads, strands) = nodes_and_loads("{foo=>[bar->[baz]] | qux->[quy]}");
        assert_eq!(nodes, 14);
        assert_eq!(loads, 4);
        assert_eq!(
            strands,
            vec![S::Expr(E {
                alternates: vec![
                    C {
                        root: None,
                        accessors: vec![
                            A::Field(ident_str!("foo")),
                            A::DOFIndex(C {
                                root: None,
                                accessors: vec![
                                    A::Field(ident_str!("bar")),
                                    A::DFIndex(C {
                                        root: None,
                                        accessors: vec![A::Field(ident_str!("baz"))],
                                    }),
                                ],
                            }),
                        ],
                    },
                    C {
                        root: None,
                        accessors: vec![
                            A::Field(ident_str!("qux")),
                            A::DFIndex(C {
                                root: None,
                                accessors: vec![A::Field(ident_str!("quy"))],
                            }),
                        ],
                    },
                ],
                transform: None,
            })]
        );
    }

    #[test]
    fn test_metering_primitive_literals() {
        let (nodes, loads, strands) =
            nodes_and_loads("{true | @0x1234 | 5678u64 | x'abcdef' | b'hello' | 'world'}");
        assert_eq!(nodes, 13);
        assert_eq!(loads, 0);
        assert_eq!(
            strands,
            vec![S::Expr(E {
                alternates: vec![
                    C {
                        root: Some(L::Bool(true)),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::Address(
                            AccountAddress::from_hex_literal("0x1234").unwrap()
                        )),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::U64(5678)),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::ByteArray(vec![0xab, 0xcd, 0xef])),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::ByteArray(b"hello".to_vec())),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::String("world".into())),
                        accessors: vec![],
                    },
                ],
                transform: None,
            })]
        );
    }

    #[test]
    fn test_metering_vectors() {
        let (nodes, loads, strands) = nodes_and_loads(
            r#"{ vector[1u8, 2u8, 3u8]
               | vector<u16>[4u16, 5u16]
               | vector<u32>
               | vector<u64>[]
               | vector<0x2::coin::Coin<0x2::sui::SUI>>
               }"#,
        );

        assert_eq!(nodes, 26);
        assert_eq!(loads, 0);
        assert_eq!(
            strands,
            vec![S::Expr(E {
                alternates: vec![
                    C {
                        root: Some(L::Vector(Box::new(Vector {
                            type_: None,
                            elements: vec![
                                C {
                                    root: Some(L::U8(1)),
                                    accessors: vec![],
                                },
                                C {
                                    root: Some(L::U8(2)),
                                    accessors: vec![],
                                },
                                C {
                                    root: Some(L::U8(3)),
                                    accessors: vec![],
                                },
                            ],
                        }))),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::Vector(Box::new(Vector {
                            type_: Some(TypeTag::U16),
                            elements: vec![
                                C {
                                    root: Some(L::U16(4)),
                                    accessors: vec![],
                                },
                                C {
                                    root: Some(L::U16(5)),
                                    accessors: vec![],
                                },
                            ],
                        }))),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::Vector(Box::new(Vector {
                            type_: Some(TypeTag::U32),
                            elements: vec![],
                        }))),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::Vector(Box::new(Vector {
                            type_: Some(TypeTag::U64),
                            elements: vec![],
                        }))),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::Vector(Box::new(Vector {
                            type_: Some(
                                TypeTag::from_str("0x2::coin::Coin<0x2::sui::SUI>").unwrap()
                            ),
                            elements: vec![],
                        }))),
                        accessors: vec![],
                    },
                ],
                transform: None,
            })]
        );
    }

    #[test]
    fn test_metering_datatypes() {
        let (nodes, loads, strands) = nodes_and_loads(
            r#"{ 0x1::string::String(42u64, 'foo', vector[1u256, 2u256, 3u256])
               | 0x2::coin::Coin<0x2::sui::SUI>::Foo#1 { balance: 100u64 } }"#,
        );

        assert_eq!(nodes, 22);
        assert_eq!(loads, 0);
        assert_eq!(
            strands,
            vec![S::Expr(E {
                alternates: vec![
                    C {
                        root: Some(L::Struct(Box::new(Struct {
                            type_: StructTag::from_str("0x1::string::String").unwrap(),
                            fields: Fields::Positional(vec![
                                C {
                                    root: Some(L::U64(42)),
                                    accessors: vec![],
                                },
                                C {
                                    root: Some(L::String("foo".into())),
                                    accessors: vec![],
                                },
                                C {
                                    root: Some(L::Vector(Box::new(Vector {
                                        type_: None,
                                        elements: vec![
                                            C {
                                                root: Some(L::U256(1u64.into())),
                                                accessors: vec![],
                                            },
                                            C {
                                                root: Some(L::U256(2u64.into())),
                                                accessors: vec![],
                                            },
                                            C {
                                                root: Some(L::U256(3u64.into())),
                                                accessors: vec![],
                                            },
                                        ],
                                    }))),
                                    accessors: vec![],
                                },
                            ]),
                        }))),
                        accessors: vec![],
                    },
                    C {
                        root: Some(L::Enum(Box::new(Enum {
                            type_: StructTag::from_str("0x2::coin::Coin<0x2::sui::SUI>").unwrap(),
                            variant_name: Some("Foo"),
                            variant_index: 1,
                            fields: Fields::Named(vec![(
                                "balance",
                                C {
                                    root: Some(L::U64(100)),
                                    accessors: vec![],
                                },
                            )]),
                        }))),
                        accessors: vec![],
                    },
                ],
                transform: None,
            })]
        );
    }

    #[test]
    fn test_metering_depth_text() {
        let src = "foo bar";
        assert!(parse_with_depth(0, src).is_ok());
    }

    #[test]
    fn test_metering_depth_expression() {
        let src = "{foo}";
        assert!(parse_with_depth(0, src).is_err());
        assert!(parse_with_depth(1, src).is_ok());
    }

    #[test]
    fn test_metering_depth_literal() {
        let src = "{0x42::foo::Bar { value: 0x1::option::Option<u64>::Some#1(vector[42u64]) }}";
        assert!(parse_with_depth(3, src).is_err());
        assert!(parse_with_depth(4, src).is_ok());
    }

    #[test]
    fn test_metering_depth_indexing() {
        let src = "{foo[bar[baz]]}";
        assert!(parse_with_depth(2, src).is_err());
        assert!(parse_with_depth(3, src).is_ok());
    }

    #[test]
    fn test_metering_depth_types() {
        let src = "{vector<0x1::foo::Bar<0x2::baz::Qux<0x2::quy::Quz>>>}";
        assert!(parse_with_depth(3, src).is_err());
        assert!(parse_with_depth(4, src).is_ok());
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
        assert_snapshot!(strands(r#"{foo | bar | baz :str}"#));
    }

    #[test]
    fn test_expr_with_transform() {
        assert_snapshot!(strands(r#"{foo.bar.baz:str}"#));
    }

    #[test]
    fn test_address_literal() {
        assert_snapshot!(strands(r#"{@0x1 | @ 0x2 | @42 | @0x12_34_56}"#));
    }

    #[test]
    fn test_bool_literals() {
        assert_snapshot!(strands(r#"{true | false}"#));
    }

    #[test]
    fn test_numeric_literals() {
        assert_snapshot!(strands(
            "Decimal Literals: \
            { 42u8 | 1_234u16 | 56_789_012_345u64 | 678_901_234_567_890_123_456u128 | 7_890_123_456_789_012_345_678_901_234_567_890_123_456u256 } \
            Hexadecimal literals: \
            { 0x42u8 | 0x123u16 | 0x4_5678u32 | 0x90_1234_5678u64 | 0x90_1234_5678_9012_3456u128 | 0x78_9012_3456_7890_1234_5679_0123_4567_8901u256 }\
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
            "{ vector[1u8, 2u8, 3u8, foo.bar, baz[42u64]] \
             | vector<u16>[]
             | vector<u32>
             | vector< u64 > [10u64, 11u64, 12u64,]
             | vector <u128,>[ 42u128 ]
             | vector<u256>[ ] }"
        ));
    }

    #[test]
    fn test_struct_positional_literals() {
        assert_snapshot!(strands(
            "{ 0x1::string::String(42u64, 'foo', vector[1u256, 2u256, 3u256]) \
             | 0x2::coin::Coin<0x2::sui::SUI> (true, 100u32,) }"
        ));
    }

    #[test]
    fn test_struct_named_literals() {
        assert_snapshot!(strands(
            "{ 0x1::string::String { length: 42u64, value: 'foo', data: vector[1u128, 2u128, 3u128], } \
             | 0x2::coin::Coin<0x2::sui::SUI> { is_locked: true, amount: 100u32 } }"
        ));
    }

    #[test]
    fn test_enum_positional_literals() {
        assert_snapshot!(strands(
            "{ 0x1::option::Option<u64>::Some#1(42u64,) \
             | 0x1::option::Option<u32>::1(43u32) \
             | 0x1::option::Option<u16>::0() }"
        ));
    }

    #[test]
    fn test_enum_named_literals() {
        assert_snapshot!(strands(
            "{ 0x1::option::Option<u64>::Some#1 { value: 42u64, } \
             | 0x1::option::Option<u32>::1 { value: 43u32 } \
             | 0x1::option::Option<u16>::None#0 {} }"
        ));
    }

    #[test]
    fn test_struct_literal_whitespace() {
        assert_snapshot!(strands(r#"{0x1 :: coin :: Coin<0x2::sui::SUI>()}"#));
    }

    #[test]
    fn test_enum_literal_whitespace() {
        assert_snapshot!(strands(r#"{0x1::option::Option<u64>::Some # 1 (42u64)}"#));
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
        assert_snapshot!(strands(r#"{foo[bar]=>[baz].qux->[quy]}"#));
    }

    #[test]
    fn test_index_with_root() {
        assert_snapshot!(strands(r#"{true=>[foo]->[bar].baz}"#));
    }

    #[test]
    fn test_positional_field_accessor() {
        assert_snapshot!(strands(r#"{foo.0[bar].1.baz}"#));
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
    fn test_identifier_with_leading_underscore() {
        assert_snapshot!(strands(r#"{_field | __private | _123mixed}"#));
    }

    #[test]
    fn test_identifier_bare_underscore() {
        assert_snapshot!(strands(r#"{_}"#));
    }

    #[test]
    fn test_positional_field_overflow() {
        assert_snapshot!(strands(r#"{foo.500}"#));
    }

    #[test]
    fn test_unbalanced_index() {
        assert_snapshot!(strands(r#"{foo[bar}"#));
    }

    #[test]
    fn test_arrow_missing_index() {
        assert_snapshot!(strands(r#"{foo->bar}"#));
    }

    #[test]
    fn test_double_arrow_missing_index() {
        assert_snapshot!(strands(r#"{foo=>}"#));
    }

    #[test]
    fn test_unexpected_characters() {
        assert_snapshot!(strands(r#"anything goes {? % ! 🔥}"#));
    }

    #[test]
    fn test_unexpected_characters_malformed_utf8() {
        // Create input with malformed UTF-8: '{' + first byte of multi-byte sequence without continuation
        let mut input = vec![b'{'];
        input.push(0xC3); // First byte of multi-byte UTF-8 sequence (missing continuation)
        input.push(b'}'); // Close brace
        let input_str = unsafe { std::str::from_utf8_unchecked(&input) };

        // This should generate an error message containing the malformed UTF-8,
        // which tests our safe error formatting implementation
        assert_snapshot!(strands(input_str));
    }

    #[test]
    fn test_trailing_alternate() {
        assert_snapshot!(strands(r#"{foo | bar | baz |}"#));
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
    fn test_numeric_overflow() {
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
    fn test_hex_literal_empty() {
        assert_snapshot!(strands(r#"{0x}"#));
    }

    #[test]
    fn test_hex_literal_only_underscores() {
        assert_snapshot!(strands(r#"{0x___u8}"#));
    }

    #[test]
    fn test_hex_literal_only_underscores_no_suffix() {
        assert_snapshot!(strands(r#"{0x____}"#));
    }

    #[test]
    fn test_decimal_literal_only_underscores() {
        // Looks like a decimal literal, but it's actually an identifier
        assert_snapshot!(strands(r#"{___u64}"#));
    }

    #[test]
    fn test_vector_literal_trailing() {
        assert_snapshot!(strands(r#"{vector[1u64, 2u64, 3u64"#));
    }

    #[test]
    fn test_vector_literal_arity() {
        assert_snapshot!(strands(r#"{vector<u8, u64>[1u8, 2u8, 3u8]}"#));
    }

    #[test]
    fn test_vector_literal_empty_no_type() {
        assert_snapshot!(strands(r#"{vector[]}"#));
    }

    #[test]
    fn test_vector_literal_empty_angles() {
        assert_snapshot!(strands(r#"{vector<>[1u8]}"#));
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
        assert_snapshot!(strands(r#"{vector<u64>[1u64 2u64]}"#));
    }

    #[test]
    fn test_vector_element_error() {
        assert_snapshot!(strands(r#"{vector<u64>[1u64, 2u64, foo . 'bar']"#));
    }

    #[test]
    fn test_type_param_no_comma() {
        assert_snapshot!(strands(r#"{vector<0x2::table::Table<u64 u64>>}"#));
    }

    #[test]
    fn test_struct_literal_positional_trailing() {
        assert_snapshot!(strands(
            r#"{0x1::string::String(42u64, 'foo', vector[1u64, 2u64, 3u64]"#
        ));
    }

    #[test]
    fn test_struct_literal_named_trailing() {
        assert_snapshot!(strands(
            r#"{0x1::string::String { length: 42u64, value: 'foo', data: vector[1u64, 2u64, 3u64]"#,
        ));
    }

    #[test]
    fn test_enum_literal_positional_trailing() {
        assert_snapshot!(strands(
            r#"{0x1::option::Option<u64>::Some#1(42u64, 43u64, 44u64}"#
        ));
    }

    #[test]
    fn test_enum_literal_named_trailing() {
        assert_snapshot!(strands(
            r#"{0x1::option::Option<u64>::Some#1 { value: 42u16, other: 43u32,"#,
        ));
    }

    #[test]
    fn test_struct_hybrid_positional_named() {
        assert_snapshot!(strands(
            r#"{0x1::string::String(length: 42u64, value: 'foo', data: vector[1u64, 2u64, 3u64])}"#
        ));
    }

    #[test]
    fn test_struct_hybrid_named_positional() {
        assert_snapshot!(strands(
            r#"{0x1::string::String { 42u64, 'foo', vector[1u64, 2u64, 3u64] }}"#
        ));
    }

    #[test]
    fn test_enum_missing_index() {
        assert_snapshot!(strands(r#"{0x1::option::Option<u64>::Some(42u64)}"#));
    }

    #[test]
    fn test_enum_variant_overflow() {
        assert_snapshot!(strands(r#"{0x1::option::Option<u64>::Some#70000(42u64)}"#));
    }
}
