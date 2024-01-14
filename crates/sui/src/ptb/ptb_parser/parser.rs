// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::ptb::ptb::PTBCommand;
use move_command_line_common::{
    address::NumericalAddress,
    parser::{parse_u128, parse_u16, parse_u256, parse_u32, parse_u64, parse_u8, Parser, Token},
    types::{ParsedType, TypeToken},
};
use move_core_types::{identifier::Identifier, runtime_value::MoveValue};
use std::{borrow::BorrowMut, marker::PhantomData, str::FromStr};

use crate::ptb::ptb_parser::argument_token::ArgumentToken;
use anyhow::{anyhow, bail, Context, Result};

use super::command_token::CommandToken;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ParsedPTBCommand<T> {
    pub name: CommandToken,
    pub args: Vec<T>,
}

impl ParsedPTBCommand<Argument> {
    pub fn parse(cmd: &PTBCommand) -> Result<Self> {
        let name = CommandToken::from_str(&cmd.name)?;
        match name {
            CommandToken::FileEnd
            | CommandToken::FileStart
            | CommandToken::Publish
            | CommandToken::Upgrade => {
                if cmd.values.len() != 1 {
                    bail!(
                        "Invalid command -- expected 1 argument, got {}",
                        cmd.values.len()
                    );
                }
                return Ok(Self {
                    name,
                    args: vec![Argument::String(cmd.values[0].clone())],
                });
            }
            _ => (),
        }
        let args = cmd
            .values
            .iter()
            .map(|v| Argument::parse_values(&v))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(Self { name, args })
    }
}

pub struct ValueParser<
    'a,
    I: Iterator<Item = (ArgumentToken, &'a str)>,
    P: BorrowMut<Parser<'a, ArgumentToken, I>>,
> {
    inner: P,
    _a: PhantomData<&'a ()>,
    _i: PhantomData<I>,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum Argument {
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(move_core_types::u256::U256),
    Identifier(Identifier),
    Address(NumericalAddress),
    String(String),
    Vector(Vec<Argument>),
    Array(Vec<Argument>),
    Option(Option<Box<Argument>>),
    ModuleAccess {
        address: NumericalAddress,
        module_name: Identifier,
        function_name: Identifier,
    },
    TyArgs(Vec<ParsedType>),
}

impl Argument {
    pub fn parse_values(s: &str) -> Result<Vec<Self>> {
        let tokens: Vec<_> = ArgumentToken::tokenize(s)?;
        let mut parser = ValueParser::new(tokens);
        let res = parser.parse_arguments()?;
        if let Ok((_, contents)) = parser.inner().advance_any() {
            bail!("Expected end of token stream. Got: {}", contents)
        }
        Ok(res)
    }

    pub fn is_lifted(&self) -> bool {
        match self {
            Argument::Bool(_)
            | Argument::U8(_)
            | Argument::U16(_)
            | Argument::U32(_)
            | Argument::U64(_)
            | Argument::U128(_)
            | Argument::U256(_)
            | Argument::Identifier(_)
            | Argument::Address(_)
            | Argument::String(_)
            | Argument::Vector(_)
            | Argument::Option(_) => true,
            Argument::ModuleAccess { .. } | Argument::TyArgs(_) | Argument::Array(_) => false,
        }
    }

    pub fn into_move_value_opt(&self) -> Result<MoveValue> {
        Ok(match self {
            Argument::Bool(b) => MoveValue::Bool(*b),
            Argument::U8(u) => MoveValue::U8(*u),
            Argument::U16(u) => MoveValue::U16(*u),
            Argument::U32(u) => MoveValue::U32(*u),
            Argument::U64(u) => MoveValue::U64(*u),
            Argument::U128(u) => MoveValue::U128(*u),
            Argument::U256(u) => MoveValue::U256(*u),
            Argument::Address(a) => MoveValue::Address(a.into_inner()),
            Argument::Vector(vs) => MoveValue::Vector(
                vs.iter()
                    .map(|v| v.into_move_value_opt())
                    .collect::<Result<Vec<_>>>()?,
            ),
            Argument::String(s) => {
                MoveValue::Vector(s.bytes().into_iter().map(MoveValue::U8).collect::<Vec<_>>())
            }
            Argument::Option(o) => {
                if let Some(v) = o {
                    MoveValue::Vector(vec![v.as_ref().into_move_value_opt()?])
                } else {
                    MoveValue::Vector(vec![])
                }
            }
            Argument::Identifier(_)
            | Argument::Array(_)
            | Argument::ModuleAccess { .. }
            | Argument::TyArgs(_) => bail!("Cannot convert {:?} to MoveValue", self),
        })
    }
}

impl<'a, I: Iterator<Item = (ArgumentToken, &'a str)>>
    ValueParser<'a, I, Parser<'a, ArgumentToken, I>>
{
    pub fn new<T: IntoIterator<Item = (ArgumentToken, &'a str), IntoIter = I>>(v: T) -> Self {
        Self::from_parser(Parser::new(v))
    }
}

impl<'a, I, P> ValueParser<'a, I, P>
where
    I: Iterator<Item = (ArgumentToken, &'a str)>,
    P: BorrowMut<Parser<'a, ArgumentToken, I>>,
{
    pub fn from_parser(inner: P) -> Self {
        Self {
            inner,
            _a: PhantomData,
            _i: PhantomData,
        }
    }

    pub fn parse_arguments(&mut self) -> Result<Vec<Argument>> {
        let args = self.inner().parse_list(
            |p| ValueParser::from_parser(p).parse_argument(),
            ArgumentToken::Whitespace,
            /* not checked */ ArgumentToken::Void,
            /* allow_trailing_delim */ true,
        )?;
        Ok(args)
    }

    fn parse_address(contents: &str) -> Result<NumericalAddress> {
        NumericalAddress::parse_str(contents).map_err(|s| {
            anyhow!(
                "Failed to parse numerical address '{}'. Got error: {}",
                contents,
                s
            )
        })
    }

    pub fn parse_argument(&mut self) -> Result<Argument> {
        use super::argument_token::ArgumentToken as Tok;
        use Argument as V;
        Ok(match self.inner().advance_any()? {
            (Tok::Ident, "true") => V::Bool(true),
            (Tok::Ident, "false") => V::Bool(false),
            (Tok::Number, contents) if matches!(self.inner().peek_tok(), Some(Tok::ColonColon)) => {
                let address = Self::parse_address(contents)?;
                self.inner().advance(Tok::ColonColon)?;
                let module_name = Identifier::new(self.inner().advance(Tok::Ident)?)?;
                self.inner().advance(Tok::ColonColon)?;
                let function_name = Identifier::new(self.inner().advance(Tok::Ident)?)?;
                V::ModuleAccess {
                    address,
                    module_name,
                    function_name,
                }
            }
            (Tok::Number, contents) => {
                let num = u64::from_str(contents).context("Invalid number")?;
                V::U64(num)
            }
            (Tok::NumberTyped, contents) => {
                if let Some(s) = contents.strip_suffix("u8") {
                    let (u, _) = parse_u8(s)?;
                    V::U8(u)
                } else if let Some(s) = contents.strip_suffix("u16") {
                    let (u, _) = parse_u16(s)?;
                    V::U16(u)
                } else if let Some(s) = contents.strip_suffix("u32") {
                    let (u, _) = parse_u32(s)?;
                    V::U32(u)
                } else if let Some(s) = contents.strip_suffix("u64") {
                    let (u, _) = parse_u64(s)?;
                    V::U64(u)
                } else if let Some(s) = contents.strip_suffix("u128") {
                    let (u, _) = parse_u128(s)?;
                    V::U128(u)
                } else {
                    let (u, _) = parse_u256(contents.strip_suffix("u256").unwrap())?;
                    V::U256(u)
                }
            }
            (Tok::At, _) => {
                let (_, contents) = self.inner().advance_any()?;
                let address = Self::parse_address(contents)?;
                V::Address(address)
            }
            (Tok::Some_, _) => {
                self.inner().advance(Tok::LParen)?;
                let arg = self.parse_argument()?;
                self.inner().advance(Tok::RParen)?;
                V::Option(Some(Box::new(arg)))
            }
            (Tok::None_, _) => V::Option(None),
            (Tok::DoubleQuote, contents) => V::String(contents.to_owned()),
            (Tok::SingleQuote, contents) => V::String(contents.to_owned()),
            (Tok::Vector, _) => {
                self.inner().advance(Tok::LBracket)?;
                let values = self.inner().parse_list(
                    |p| ValueParser::from_parser(p).parse_argument(),
                    ArgumentToken::Comma,
                    Tok::RBracket,
                    /* allow_trailing_delim */ true,
                )?;
                self.inner().advance(Tok::RBracket)?;
                V::Vector(values)
            }
            (Tok::LBracket, _) => {
                let values = self.inner().parse_list(
                    |p| ValueParser::from_parser(p).parse_argument(),
                    ArgumentToken::Comma,
                    Tok::RBracket,
                    /* allow_trailing_delim */ true,
                )?;
                self.inner().advance(Tok::RBracket)?;
                V::Array(values)
            }
            (Tok::Ident, contents) => V::Identifier(Identifier::new(contents)?),
            (Tok::TypeArgString, contents) => {
                let type_tokens: Vec<_> = TypeToken::tokenize(contents)?
                    .into_iter()
                    .filter(|(tok, _)| !tok.is_whitespace())
                    .collect();
                let mut parser = Parser::new(type_tokens);
                parser.advance(TypeToken::Lt)?;
                let res =
                    parser.parse_list(|p| p.parse_type(), TypeToken::Comma, TypeToken::Gt, true)?;
                parser.advance(TypeToken::Gt)?;
                if let Ok((_, contents)) = parser.advance_any() {
                    bail!("Expected end of token stream. Got: {}", contents)
                }
                V::TyArgs(res)
            }
            x => bail!("unexpected token {:?}, expected argument", x),
        })
    }

    pub fn inner(&mut self) -> &mut Parser<'a, ArgumentToken, I> {
        self.inner.borrow_mut()
    }
}

mod tests {
    use super::*;

    #[test]
    fn parse_value() {
        let values = vec![
            "true",
            "false",
            "1",
            "1u8",
            "1u16",
            "1u32",
            "1u64",
            "some(ident)",
            "some(123)",
            "some(@0x0)",
            "none",
        ];
        for s in &values {
            assert!(dbg!(Argument::parse_values(s)).is_ok());
        }
    }

    #[test]
    fn parse_values() {
        let values = vec![
            "true @0x0 false 1 1u8",
            "true @0x0 false 1 1u8 vector_ident another ident",
            "true @0x0 false 1 1u8 some_ident another ident some(123) none",
            "true @0x0 false 1 1u8 some_ident another ident some(123) none vector[] [] [vector[]] [vector[1]] [vector[1,2]] [vector[1,2,]]",
        ];
        for s in &values {
            assert!(dbg!(Argument::parse_values(s)).is_ok());
        }
    }

    #[test]
    fn parse_address() {
        let values = vec!["@0x0", "@1234"];
        for s in &values {
            assert!(dbg!(Argument::parse_values(s)).is_ok());
        }
    }

    #[test]
    fn parse_string() {
        let values = vec!["\"hello world\"", "'hello world'"];
        for s in &values {
            assert!(dbg!(Argument::parse_values(s)).is_ok());
        }
    }

    // TODO: handle Whitespace within vectors and arrays
    #[test]
    fn parse_vector() {
        let values = vec!["vector[]", "vector[1]", "vector[1,2]", "vector[1,2,]"];
        for s in &values {
            assert!(dbg!(Argument::parse_values(s)).is_ok());
        }
    }

    // TODO: handle Whitespace within vectors and arrays
    #[test]
    fn parse_array() {
        let values = vec!["[]", "[1]", "[1,2]", "[1,2,]"];
        for s in &values {
            assert!(dbg!(Argument::parse_values(s)).is_ok());
        }
    }

    #[test]
    fn module_access() {
        let values = vec!["123::b::c", "0x0::b::c"];
        for s in &values {
            assert!(dbg!(Argument::parse_values(s)).is_ok());
        }
    }

    #[test]
    fn type_args() {
        let values = vec!["<u64>", "<0x0::b::c>", "<0x0::b::c, 1234::g::f>"];
        for s in &values {
            assert!(dbg!(Argument::parse_values(s)).is_ok());
        }
    }

    #[test]
    fn move_call() {
        let values = vec![
            "0x0::M::f",
            "<u64, 123::a::f<456::b::c>>",
            "1 2u32 vector[]",
        ];
        for s in &values {
            assert!(dbg!(Argument::parse_values(s)).is_ok());
        }
    }
}
