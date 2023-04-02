// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    address::{NumericalAddress, ParsedAddress},
    types::{ParsedStructType, ParsedType, TypeToken},
    values::{ParsableValue, ParsedValue, ValueToken},
};
use anyhow::{anyhow, bail, Result};
use move_core_types::{
    account_address::AccountAddress,
    u256::{U256FromStrError, U256},
};
use num_bigint::BigUint;
use std::{collections::BTreeMap, fmt::Display, iter::Peekable, num::ParseIntError};

pub trait Token: Display + Copy + Eq {
    fn is_whitespace(&self) -> bool;
    fn next_token(s: &str) -> Result<Option<(Self, usize)>>;
    fn tokenize(mut s: &str) -> Result<Vec<(Self, &str)>> {
        let mut v = vec![];
        while let Some((tok, n)) = Self::next_token(s)? {
            v.push((tok, &s[..n]));
            s = &s[n..];
        }
        Ok(v)
    }
}

pub struct Parser<'a, Tok: Token, I: Iterator<Item = (Tok, &'a str)>> {
    it: Peekable<I>,
}

impl ParsedType {
    pub fn parse(s: &str) -> Result<ParsedType> {
        parse(s, |parser| parser.parse_type())
    }
}

impl ParsedStructType {
    pub fn parse(s: &str) -> Result<ParsedStructType> {
        let ty = parse(s, |parser| parser.parse_type())
            .map_err(|e| anyhow!("Invalid struct type: {}. Got error: {}", s, e))?;
        match ty {
            ParsedType::Struct(s) => Ok(s),
            _ => bail!("Invalid struct type: {}", s),
        }
    }
}

impl ParsedAddress {
    pub fn parse(s: &str) -> Result<ParsedAddress> {
        parse(s, |parser| parser.parse_address())
    }
}

impl<Extra: ParsableValue> ParsedValue<Extra> {
    pub fn parse(s: &str) -> Result<ParsedValue<Extra>> {
        parse(s, |parser| parser.parse_value())
    }
}

fn parse<'a, Tok: Token, R>(
    s: &'a str,
    f: impl FnOnce(&mut Parser<'a, Tok, std::vec::IntoIter<(Tok, &'a str)>>) -> Result<R>,
) -> Result<R> {
    let tokens: Vec<_> = Tok::tokenize(s)?
        .into_iter()
        .filter(|(tok, _)| !tok.is_whitespace())
        .collect();
    let mut parser = Parser::new(tokens);
    let res = f(&mut parser)?;
    if let Ok((_, contents)) = parser.advance_any() {
        bail!("Expected end of token stream. Got: {}", contents)
    }
    Ok(res)
}

impl<'a, Tok: Token, I: Iterator<Item = (Tok, &'a str)>> Parser<'a, Tok, I> {
    pub fn new<T: IntoIterator<Item = (Tok, &'a str), IntoIter = I>>(v: T) -> Self {
        Self {
            it: v.into_iter().peekable(),
        }
    }

    pub fn advance_any(&mut self) -> Result<(Tok, &'a str)> {
        match self.it.next() {
            Some(tok) => Ok(tok),
            None => bail!("unexpected end of tokens"),
        }
    }

    pub fn advance(&mut self, expected_token: Tok) -> Result<&'a str> {
        let (t, contents) = self.advance_any()?;
        if t != expected_token {
            bail!("expected token {}, got {}", expected_token, t)
        }
        Ok(contents)
    }

    pub fn peek(&mut self) -> Option<(Tok, &'a str)> {
        self.it.peek().copied()
    }

    pub fn peek_tok(&mut self) -> Option<Tok> {
        self.it.peek().map(|(tok, _)| *tok)
    }

    pub fn parse_list<R>(
        &mut self,
        parse_list_item: impl Fn(&mut Self) -> Result<R>,
        delim: Tok,
        end_token: Tok,
        allow_trailing_delim: bool,
    ) -> Result<Vec<R>> {
        let is_end =
            |tok_opt: Option<Tok>| -> bool { tok_opt.map(|tok| tok == end_token).unwrap_or(true) };
        let mut v = vec![];
        while !is_end(self.peek_tok()) {
            v.push(parse_list_item(self)?);
            if is_end(self.peek_tok()) {
                break;
            }
            self.advance(delim)?;
            if is_end(self.peek_tok()) && allow_trailing_delim {
                break;
            }
        }
        Ok(v)
    }
}

impl<'a, I: Iterator<Item = (TypeToken, &'a str)>> Parser<'a, TypeToken, I> {
    pub fn parse_type(&mut self) -> Result<ParsedType> {
        let (tok, contents) = self.advance_any()?;
        Ok(match (tok, contents) {
            (TypeToken::Ident, "u8") => ParsedType::U8,
            (TypeToken::Ident, "u16") => ParsedType::U16,
            (TypeToken::Ident, "u32") => ParsedType::U32,
            (TypeToken::Ident, "u64") => ParsedType::U64,
            (TypeToken::Ident, "u128") => ParsedType::U128,
            (TypeToken::Ident, "u256") => ParsedType::U256,
            (TypeToken::Ident, "bool") => ParsedType::Bool,
            (TypeToken::Ident, "address") => ParsedType::Address,
            (TypeToken::Ident, "signer") => ParsedType::Signer,
            (TypeToken::Ident, "vector") => {
                self.advance(TypeToken::Lt)?;
                let ty = self.parse_type()?;
                self.advance(TypeToken::Gt)?;
                ParsedType::Vector(Box::new(ty))
            }
            (TypeToken::Ident, _) | (TypeToken::AddressIdent, _) => {
                let addr_tok = match tok {
                    TypeToken::Ident => ValueToken::Ident,
                    TypeToken::AddressIdent => ValueToken::Number,
                    _ => unreachable!(),
                };
                let address = parse_address_impl(addr_tok, contents)?;
                self.advance(TypeToken::ColonColon)?;
                let module_contents = self.advance(TypeToken::Ident)?;
                self.advance(TypeToken::ColonColon)?;
                let struct_contents = self.advance(TypeToken::Ident)?;
                let type_args = match self.peek_tok() {
                    Some(TypeToken::Lt) => {
                        self.advance(TypeToken::Lt)?;
                        let type_args = self.parse_list(
                            |parser| parser.parse_type(),
                            TypeToken::Comma,
                            TypeToken::Gt,
                            true,
                        )?;
                        self.advance(TypeToken::Gt)?;
                        type_args
                    }
                    _ => vec![],
                };
                ParsedType::Struct(ParsedStructType {
                    address,
                    module: module_contents.to_owned(),
                    name: struct_contents.to_owned(),
                    type_args,
                })
            }
            _ => bail!("unexpected token {}, expected type", tok),
        })
    }
}

impl<'a, I: Iterator<Item = (ValueToken, &'a str)>> Parser<'a, ValueToken, I> {
    pub fn parse_value<Extra: ParsableValue>(&mut self) -> Result<ParsedValue<Extra>> {
        if let Some(extra) = Extra::parse_value(self) {
            return Ok(ParsedValue::Custom(extra?));
        }
        let (tok, contents) = self.advance_any()?;
        Ok(match tok {
            ValueToken::Number if !matches!(self.peek_tok(), Some(ValueToken::ColonColon)) => {
                let (u, _) = parse_u256(contents)?;
                ParsedValue::InferredNum(u)
            }
            ValueToken::NumberTyped => {
                if let Some(s) = contents.strip_suffix("u8") {
                    let (u, _) = parse_u8(s)?;
                    ParsedValue::U8(u)
                } else if let Some(s) = contents.strip_suffix("u16") {
                    let (u, _) = parse_u16(s)?;
                    ParsedValue::U16(u)
                } else if let Some(s) = contents.strip_suffix("u32") {
                    let (u, _) = parse_u32(s)?;
                    ParsedValue::U32(u)
                } else if let Some(s) = contents.strip_suffix("u64") {
                    let (u, _) = parse_u64(s)?;
                    ParsedValue::U64(u)
                } else if let Some(s) = contents.strip_suffix("u128") {
                    let (u, _) = parse_u128(s)?;
                    ParsedValue::U128(u)
                } else {
                    let (u, _) = parse_u256(contents.strip_suffix("u256").unwrap())?;
                    ParsedValue::U256(u)
                }
            }
            ValueToken::True => ParsedValue::Bool(true),
            ValueToken::False => ParsedValue::Bool(false),

            ValueToken::ByteString => {
                let contents = contents
                    .strip_prefix("b\"")
                    .unwrap()
                    .strip_suffix('\"')
                    .unwrap();
                ParsedValue::Vector(
                    contents
                        .as_bytes()
                        .iter()
                        .copied()
                        .map(ParsedValue::U8)
                        .collect(),
                )
            }
            ValueToken::HexString => {
                let contents = contents
                    .strip_prefix("x\"")
                    .unwrap()
                    .strip_suffix('\"')
                    .unwrap()
                    .to_ascii_lowercase();
                ParsedValue::Vector(
                    hex::decode(contents)
                        .unwrap()
                        .into_iter()
                        .map(ParsedValue::U8)
                        .collect(),
                )
            }
            ValueToken::Utf8String => {
                let contents = contents
                    .strip_prefix('\"')
                    .unwrap()
                    .strip_suffix('\"')
                    .unwrap();
                ParsedValue::Vector(
                    contents
                        .as_bytes()
                        .iter()
                        .copied()
                        .map(ParsedValue::U8)
                        .collect(),
                )
            }

            ValueToken::AtSign => ParsedValue::Address(self.parse_address()?),

            ValueToken::Ident if contents == "vector" => {
                self.advance(ValueToken::LBracket)?;
                let values = self.parse_list(
                    |parser| parser.parse_value(),
                    ValueToken::Comma,
                    ValueToken::RBracket,
                    true,
                )?;
                self.advance(ValueToken::RBracket)?;
                ParsedValue::Vector(values)
            }

            ValueToken::Number | ValueToken::Ident => {
                let addr_ident = parse_address_impl(tok, contents)?;
                self.advance(ValueToken::ColonColon)?;
                let module_name = self.advance(ValueToken::Ident)?.to_owned();
                self.advance(ValueToken::ColonColon)?;
                let struct_name = self.advance(ValueToken::Ident)?.to_owned();
                self.advance(ValueToken::LBrace)?;
                let values_vec = self.parse_list(
                    |parser| {
                        let field = parser.advance(ValueToken::Ident)?.to_owned();
                        parser.advance(ValueToken::Colon)?;
                        let value = parser.parse_value()?;
                        Ok((field, value))
                    },
                    ValueToken::Comma,
                    ValueToken::RBracket,
                    true,
                )?;
                self.advance(ValueToken::RBrace)?;
                let mut values = BTreeMap::new();
                for (field, value) in values_vec {
                    if let Some(_prev) = values.insert(field.clone(), value) {
                        // TODO should this be done in here? Seems useful for most tools though...
                        bail!("Duplicate field binding for field: {}", field)
                    }
                }
                ParsedValue::Struct(addr_ident, module_name, struct_name, values)
            }

            _ => bail!("unexpected token {}, expected type", tok),
        })
    }

    pub fn parse_address(&mut self) -> Result<ParsedAddress> {
        let (tok, contents) = self.advance_any()?;
        parse_address_impl(tok, contents)
    }
}

pub fn parse_address_impl(tok: ValueToken, contents: &str) -> Result<ParsedAddress> {
    Ok(match tok {
        ValueToken::Number => {
            ParsedAddress::Numerical(NumericalAddress::parse_str(contents).map_err(|s| {
                anyhow!(
                    "Failed to parse numerical address '{}'. Got error: {}",
                    contents,
                    s
                )
            })?)
        }
        ValueToken::Ident => ParsedAddress::Named(contents.to_owned()),
        _ => bail!("unexpected token {}, expected identifier or number", tok),
    })
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Copy)]
#[repr(u32)]
/// Number format enum, the u32 value represents the base
pub enum NumberFormat {
    Decimal = 10,
    Hex = 16,
}

// Determines the base of the number literal, depending on the prefix
pub(crate) fn determine_num_text_and_base(s: &str) -> (&str, NumberFormat) {
    match s.strip_prefix("0x") {
        Some(s_hex) => (s_hex, NumberFormat::Hex),
        None => (s, NumberFormat::Decimal),
    }
}

// Parse a u8 from a decimal or hex encoding
pub fn parse_u8(s: &str) -> Result<(u8, NumberFormat), ParseIntError> {
    let (txt, base) = determine_num_text_and_base(s);
    Ok((
        u8::from_str_radix(&txt.replace('_', ""), base as u32)?,
        base,
    ))
}

// Parse a u16 from a decimal or hex encoding
pub fn parse_u16(s: &str) -> Result<(u16, NumberFormat), ParseIntError> {
    let (txt, base) = determine_num_text_and_base(s);
    Ok((
        u16::from_str_radix(&txt.replace('_', ""), base as u32)?,
        base,
    ))
}

// Parse a u32 from a decimal or hex encoding
pub fn parse_u32(s: &str) -> Result<(u32, NumberFormat), ParseIntError> {
    let (txt, base) = determine_num_text_and_base(s);
    Ok((
        u32::from_str_radix(&txt.replace('_', ""), base as u32)?,
        base,
    ))
}

// Parse a u64 from a decimal or hex encoding
pub fn parse_u64(s: &str) -> Result<(u64, NumberFormat), ParseIntError> {
    let (txt, base) = determine_num_text_and_base(s);
    Ok((
        u64::from_str_radix(&txt.replace('_', ""), base as u32)?,
        base,
    ))
}

// Parse a u128 from a decimal or hex encoding
pub fn parse_u128(s: &str) -> Result<(u128, NumberFormat), ParseIntError> {
    let (txt, base) = determine_num_text_and_base(s);
    Ok((
        u128::from_str_radix(&txt.replace('_', ""), base as u32)?,
        base,
    ))
}

// Parse a u256 from a decimal or hex encoding
pub fn parse_u256(s: &str) -> Result<(U256, NumberFormat), U256FromStrError> {
    let (txt, base) = determine_num_text_and_base(s);
    Ok((
        U256::from_str_radix(&txt.replace('_', ""), base as u32)?,
        base,
    ))
}

// Parse an address from a decimal or hex encoding
pub fn parse_address_number(s: &str) -> Option<([u8; AccountAddress::LENGTH], NumberFormat)> {
    let (txt, base) = determine_num_text_and_base(s);
    let parsed = BigUint::parse_bytes(
        txt.as_bytes(),
        match base {
            NumberFormat::Hex => 16,
            NumberFormat::Decimal => 10,
        },
    )?;
    let bytes = parsed.to_bytes_be();
    if bytes.len() > AccountAddress::LENGTH {
        return None;
    }
    let mut result = [0u8; AccountAddress::LENGTH];
    result[(AccountAddress::LENGTH - bytes.len())..].clone_from_slice(&bytes);
    Some((result, base))
}

#[cfg(test)]
mod tests {
    use crate::{
        address::{NumericalAddress, ParsedAddress},
        types::{ParsedStructType, ParsedType},
        values::ParsedValue,
    };
    use move_core_types::{account_address::AccountAddress, u256::U256};

    #[allow(clippy::unreadable_literal)]
    #[test]
    fn tests_parse_value_positive() {
        use ParsedValue as V;
        let cases: &[(&str, V)] = &[
            ("  0u8", V::U8(0)),
            ("0u8", V::U8(0)),
            ("0xF_Fu8", V::U8(255)),
            ("0xF__FF__Eu16", V::U16(u16::MAX - 1)),
            ("0xFFF_FF__FF_Cu32", V::U32(u32::MAX - 3)),
            ("255u8", V::U8(255)),
            ("255u256", V::U256(U256::from(255u64))),
            ("0", V::InferredNum(U256::from(0u64))),
            ("0123", V::InferredNum(U256::from(123u64))),
            ("0xFF", V::InferredNum(U256::from(0xFFu64))),
            ("0xF_F", V::InferredNum(U256::from(0xFFu64))),
            ("0xFF__", V::InferredNum(U256::from(0xFFu64))),
            (
                "0x12_34__ABCD_FF",
                V::InferredNum(U256::from(0x1234ABCDFFu64)),
            ),
            ("0u64", V::U64(0)),
            ("0x0u64", V::U64(0)),
            (
                "18446744073709551615",
                V::InferredNum(U256::from(18446744073709551615u128)),
            ),
            ("18446744073709551615u64", V::U64(18446744073709551615)),
            ("0u128", V::U128(0)),
            ("1_0u8", V::U8(1_0)),
            ("10_u8", V::U8(10)),
            ("1_000u64", V::U64(1_000)),
            ("1_000", V::InferredNum(U256::from(1_000u32))),
            ("1_0_0_0u64", V::U64(1_000)),
            ("1_000_000u128", V::U128(1_000_000)),
            (
                "340282366920938463463374607431768211455u128",
                V::U128(340282366920938463463374607431768211455),
            ),
            ("true", V::Bool(true)),
            ("false", V::Bool(false)),
            (
                "@0x0",
                V::Address(ParsedAddress::Numerical(NumericalAddress::new(
                    AccountAddress::from_hex_literal("0x0")
                        .unwrap()
                        .into_bytes(),
                    crate::parser::NumberFormat::Hex,
                ))),
            ),
            (
                "@0",
                V::Address(ParsedAddress::Numerical(NumericalAddress::new(
                    AccountAddress::from_hex_literal("0x0")
                        .unwrap()
                        .into_bytes(),
                    crate::parser::NumberFormat::Hex,
                ))),
            ),
            (
                "@0x54afa3526",
                V::Address(ParsedAddress::Numerical(NumericalAddress::new(
                    AccountAddress::from_hex_literal("0x54afa3526")
                        .unwrap()
                        .into_bytes(),
                    crate::parser::NumberFormat::Hex,
                ))),
            ),
            (
                "b\"hello\"",
                V::Vector("hello".as_bytes().iter().copied().map(V::U8).collect()),
            ),
            ("x\"7fff\"", V::Vector(vec![V::U8(0x7f), V::U8(0xff)])),
            ("x\"\"", V::Vector(vec![])),
            ("x\"00\"", V::Vector(vec![V::U8(0x00)])),
            (
                "x\"deadbeef\"",
                V::Vector(vec![V::U8(0xde), V::U8(0xad), V::U8(0xbe), V::U8(0xef)]),
            ),
        ];

        for (s, expected) in cases {
            assert_eq!(&ParsedValue::parse(s).unwrap(), expected)
        }
    }

    #[test]
    fn tests_parse_value_negative() {
        /// Test cases for the parser that should always fail.
        const PARSE_VALUE_NEGATIVE_TEST_CASES: &[&str] = &[
            "-3",
            "0u42",
            "0u645",
            "0u64x",
            "0u6 4",
            "0u",
            "_10",
            "_10_u8",
            "_10__u8",
            "10_u8__",
            "0xFF_u8_",
            "0xF_u8__",
            "0x_F_u8__",
            "_",
            "__",
            "__4",
            "_u8",
            "5_bool",
            "256u8",
            "4294967296u32",
            "65536u16",
            "18446744073709551616u64",
            "340282366920938463463374607431768211456u128",
            "340282366920938463463374607431768211456340282366920938463463374607431768211456340282366920938463463374607431768211456340282366920938463463374607431768211456u256",
            "0xg",
            "0x00g0",
            "0x",
            "0x_",
            "",
            "@@",
            "()",
            "x\"ffff",
            "x\"a \"",
            "x\" \"",
            "x\"0g\"",
            "x\"0\"",
            "garbage",
            "true3",
            "3false",
            "3 false",
            "",
            "0XFF",
            "0X0",
        ];

        for s in PARSE_VALUE_NEGATIVE_TEST_CASES {
            assert!(
                ParsedValue::<()>::parse(s).is_err(),
                "Unexpectedly succeeded in parsing: {}",
                s
            )
        }
    }

    #[test]
    fn test_type_type() {
        for s in &[
            "u64",
            "bool",
            "vector<u8>",
            "vector<vector<u64>>",
            "address",
            "signer",
            "0x1::M::S",
            "0x2::M::S_",
            "0x3::M_::S",
            "0x4::M_::S_",
            "0x00000000004::M::S",
            "0x1::M::S<u64>",
            "0x1::M::S<0x2::P::Q>",
            "vector<0x1::M::S>",
            "vector<0x1::M_::S_>",
            "vector<vector<0x1::M_::S_>>",
            "0x1::M::S<vector<u8>>",
        ] {
            assert!(ParsedType::parse(s).is_ok(), "Failed to parse type {}", s);
        }
    }

    #[test]
    fn test_parse_valid_struct_type() {
        let valid = vec![
            "0x1::Foo::Foo",
            "0x1::Foo_Type::Foo",
            "0x1::Foo_::Foo",
            "0x1::X_123::X32_",
            "0x1::Foo::Foo_Type",
            "0x1::Foo::Foo<0x1::ABC::ABC>",
            "0x1::Foo::Foo<0x1::ABC::ABC_Type>",
            "0x1::Foo::Foo<u8>",
            "0x1::Foo::Foo<u16>",
            "0x1::Foo::Foo<u32>",
            "0x1::Foo::Foo<u64>",
            "0x1::Foo::Foo<u128>",
            "0x1::Foo::Foo<u256>",
            "0x1::Foo::Foo<bool>",
            "0x1::Foo::Foo<address>",
            "0x1::Foo::Foo<signer>",
            "0x1::Foo::Foo<vector<0x1::ABC::ABC>>",
            "0x1::Foo::Foo<u8,bool>",
            "0x1::Foo::Foo<u8,   bool>",
            "0x1::Foo::Foo<u8  ,bool>",
            "0x1::Foo::Foo<u8 , bool  ,    vector<u8>,address,signer>",
            "0x1::Foo::Foo<vector<0x1::Foo::Struct<0x1::XYZ::XYZ>>>",
            "0x1::Foo::Foo<0x1::Foo::Struct<vector<0x1::XYZ::XYZ>, 0x1::Foo::Foo<vector<0x1::Foo::Struct<0x1::XYZ::XYZ>>>>>",
        ];
        for s in valid {
            assert!(
                ParsedStructType::parse(s).is_ok(),
                "Failed to parse struct {}",
                s
            );
        }
    }
}
