// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    address::{NumericalAddress, ParsedAddress},
    values::{ParsableValue, ParsedValue, ValueToken},
};
use anyhow::{anyhow, bail, Result};
use move_core_types::{
    account_address::AccountAddress,
    u256::{U256FromStrError, U256},
};
use num_bigint::BigUint;
use std::{fmt::Display, iter::Peekable, num::ParseIntError};

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

            ValueToken::Ident if contents == "struct" => {
                self.advance(ValueToken::LParen)?;
                let values = self.parse_list(
                    |parser| parser.parse_value(),
                    ValueToken::Comma,
                    ValueToken::RParen,
                    true,
                )?;
                self.advance(ValueToken::RParen)?;
                ParsedValue::Struct(values)
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
}
