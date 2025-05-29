// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::borrow::Cow;

use move_core_types::{
    account_address::AccountAddress,
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

/// Construct a set of expected tokens, (tokens that the parser tried to match against at the
/// current position).
///
/// The first argument is an optional previous `ExpectedSet`, used to chain together multiple
/// different match expressions. The remaining arguments are a list of lexeme pattern matches.
macro_rules! expected_set {
    ($prev:expr; $($kind:ident, $pat:path, $slice:tt),+) => {
        ExpectedSet {
            prev: Some(Box::new($prev)),
            tried: &[$(expected!($kind, $pat, $slice)),+],
        }
    };

    ($($kind:ident, $pat:path, $slice:tt),+) => {
        ExpectedSet {
            prev: None,
            tried: &[$(expected!($kind, $pat, $slice)),+],
        }
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
            Some(&actual) => return Err(Error::UnexpectedToken {
                actual: actual.detach(),
                expect: expected_set!($($prev;)? $($($kind, $pat, $slice),+),+),
            }),
            None => return Err(Error::UnexpectedEos {
                expect: expected_set!($($prev;)? $($($kind, $pat, $slice),+),+),
            }),
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
            Some(_) | None => Match::Tried(
                expected_set!($($prev;)? $($($kind, $pat, $slice),+),+)
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
            Match::Tried(tried) => {
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
                let addr = match_token! { self.lexer;
                    Tok(T::NumHex, _, slice) => {
                        self.lexer.next();
                        read_u256(slice, 16, "address")?
                    },

                    Tok(T::NumDec, _, slice) => {
                        self.lexer.next();
                        read_u256(slice, 10, "address")?
                    },
                };

                Literal::Address(AccountAddress::from(addr.to_be_bytes()))
            },

            Lit(T::Ident, _, "true") => {
                self.lexer.next();
                Literal::Bool(true)
            },

            Lit(T::Ident, _, "false") => {
                self.lexer.next();
                Literal::Bool(false)
            },

            Tok(T::NumDec, _, dec) => {
                self.lexer.next();
                let literal = match_token_opt! { self.lexer;
                    Lit(T::Ident, _, "u8") => { self.lexer.next(); Literal::U8(read_u8(dec, 10, "u8")?) },
                    Lit(T::Ident, _, "u16") => { self.lexer.next(); Literal::U16(read_u16(dec, 10, "u16")?) },
                    Lit(T::Ident, _, "u32") => { self.lexer.next(); Literal::U32(read_u32(dec, 10, "u32")?) },
                    Lit(T::Ident, _, "u64") => { self.lexer.next(); Literal::U64(read_u64(dec, 10, "u64")?) },
                    Lit(T::Ident, _, "u128") => { self.lexer.next(); Literal::U128(read_u128(dec, 10, "u128")?) },
                    Lit(T::Ident, _, "u256") => { self.lexer.next(); Literal::U256(read_u256(dec, 10, "u256")?) },
                };

                // If there was no explicit type suffix, assume `u64`.
                if let Match::Found(lit) = literal {
                    lit
                } else {
                    Literal::U64(read_u64(dec, 10, "u64")?)
                }
            },

            Tok(T::NumHex, _, hex) => {
                self.lexer.next();
                let literal = match_token_opt! { self.lexer;
                    Lit(T::Ident, _, "u8") => { self.lexer.next(); Literal::U8(read_u8(hex, 16, "u8")?) },
                    Lit(T::Ident, _, "u16") => { self.lexer.next(); Literal::U16(read_u16(hex, 16, "u16")?) },
                    Lit(T::Ident, _, "u32") => { self.lexer.next(); Literal::U32(read_u32(hex, 16, "u32")?) },
                    Lit(T::Ident, _, "u64") => { self.lexer.next(); Literal::U64(read_u64(hex, 16, "u64")?) },
                    Lit(T::Ident, _, "u128") => { self.lexer.next(); Literal::U128(read_u128(hex, 16, "u128")?) },
                    Lit(T::Ident, _, "u256") => { self.lexer.next(); Literal::U256(read_u256(hex, 16, "u256")?) },
                };

                // If there was no explicit type suffix, assume `u64`.
                if let Match::Found(lit) = literal {
                    lit
                } else {
                    Literal::U64(read_u64(hex, 16, "u64")?)
                }
            },

            Tok(T::String, _, slice) => {
                self.lexer.next();
                Literal::String(read_string_literal(slice))
            },

            Lit(T::Ident, _, "b") if self.lexer.peek2().is_some_and(|Lex(t, _, _)| *t == T::String) => {
                self.lexer.next();
                let Some(Lex(T::String, _, slice)) = self.lexer.next() else {
                    unreachable!("SAFETY: match guard confirms this token exists")
                };

                let output = read_string_literal(slice);
                Literal::ByteArray(output.into_owned().into_bytes())
            },

            Lit(T::Ident, _, "x") if self.lexer.peek2().is_some_and(|Lex(t, _, _)| *t == T::String) => {
                self.lexer.next();
                let Some(lex@Lex(T::String, _, slice)) = self.lexer.next() else {
                    unreachable!("SAFETY: match guard confirms this token exists")
                };

                let output = read_hex_literal(&lex, slice)?;
                Literal::ByteArray(output)
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
}
