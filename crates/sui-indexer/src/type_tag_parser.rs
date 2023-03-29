// Copyright (c) Mysten Labs, Inc.
// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, format_err, Result};
use fastcrypto::encoding::{Encoding, Hex};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{self, Identifier},
    language_storage::{StructTag, TypeTag},
};
use std::iter::Peekable;

#[derive(Eq, PartialEq, Debug)]
enum Token {
    U8Type,
    U16Type,
    U32Type,
    U64Type,
    U128Type,
    U256Type,
    BoolType,
    AddressType,
    VectorType,
    SignerType,
    Whitespace(String),
    Name(String),
    Address(String),
    U8(String),
    U16(String),
    U32(String),
    U64(String),
    U128(String),
    U256(String),

    Bytes(String),
    True,
    False,
    ColonColon,
    Lt,
    Gt,
    Comma,
    Eof,
}

impl Token {
    fn is_whitespace(&self) -> bool {
        matches!(self, Self::Whitespace(_))
    }
}

fn token_to_name(t: Token) -> Result<String> {
    Ok(match t {
        Token::U8Type => "u8".to_string(),
        Token::U16Type => "u16".to_string(),
        Token::U32Type => "u32".to_string(),
        Token::U64Type => "u64".to_string(),
        Token::U128Type => "u128".to_string(),
        Token::U256Type => "u256".to_string(),
        Token::BoolType => "bool".to_string(),
        Token::AddressType => "address".to_string(),
        Token::VectorType => "vector".to_string(),
        Token::True => "true".to_string(),
        Token::False => "false".to_string(),
        Token::SignerType => "signer".to_string(),
        Token::Name(name) => name,
        _ => bail!("Unexpected token {:?} expected a name", t),
    })
}

fn name_token(s: String) -> Token {
    match s.as_str() {
        "u8" => Token::U8Type,
        "u16" => Token::U16Type,
        "u32" => Token::U32Type,
        "u64" => Token::U64Type,
        "u128" => Token::U128Type,
        "u256" => Token::U256Type,
        "bool" => Token::BoolType,
        "address" => Token::AddressType,
        "vector" => Token::VectorType,
        "true" => Token::True,
        "false" => Token::False,
        "signer" => Token::SignerType,
        _ => Token::Name(s),
    }
}

fn next_number(initial: char, mut it: impl Iterator<Item = char>) -> Result<(Token, usize)> {
    let mut num = String::new();
    num.push(initial);
    loop {
        match it.next() {
            Some(c) if c.is_ascii_digit() || c == '_' => num.push(c),
            Some(c) if c.is_alphanumeric() => {
                let mut suffix = String::new();
                suffix.push(c);
                loop {
                    match it.next() {
                        Some(c) if c.is_ascii_alphanumeric() => suffix.push(c),
                        _ => {
                            let len = num.len() + suffix.len();
                            let tok = match suffix.as_str() {
                                "u8" => Token::U8(num),
                                "u16" => Token::U16(num),
                                "u32" => Token::U32(num),
                                "u64" => Token::U64(num),
                                "u128" => Token::U128(num),
                                "u256" => Token::U256(num),
                                _ => bail!("invalid suffix"),
                            };
                            return Ok((tok, len));
                        }
                    }
                }
            }
            _ => {
                let len = num.len();
                return Ok((Token::U64(num), len));
            }
        }
    }
}

#[allow(clippy::many_single_char_names)]
fn next_token(s: &str) -> Result<Option<(Token, usize)>> {
    let mut it = s.chars().peekable();
    match it.next() {
        None => Ok(None),
        Some(c) => Ok(Some(match c {
            '<' => (Token::Lt, 1),
            '>' => (Token::Gt, 1),
            ',' => (Token::Comma, 1),
            ':' => match it.next() {
                Some(':') => (Token::ColonColon, 2),
                _ => bail!("unrecognized token"),
            },
            '0' if it.peek() == Some(&'x') || it.peek() == Some(&'X') => {
                it.next().unwrap();
                match it.next() {
                    Some(c) if c.is_ascii_hexdigit() => {
                        let mut r = String::new();
                        r.push('0');
                        r.push('x');
                        r.push(c);
                        for c in it {
                            if c.is_ascii_hexdigit() {
                                r.push(c);
                            } else {
                                break;
                            }
                        }
                        let len = r.len();
                        (Token::Address(r), len)
                    }
                    _ => bail!("unrecognized token"),
                }
            }
            c if c.is_ascii_digit() => next_number(c, it)?,
            'b' if it.peek() == Some(&'"') => {
                it.next().unwrap();
                let mut r = String::new();
                loop {
                    match it.next() {
                        Some('"') => break,
                        Some(c) if c.is_ascii() => r.push(c),
                        _ => bail!("unrecognized token"),
                    }
                }
                let len = r.len() + 3;
                (Token::Bytes(Hex::encode(r)), len)
            }
            'x' if it.peek() == Some(&'"') => {
                it.next().unwrap();
                let mut r = String::new();
                loop {
                    match it.next() {
                        Some('"') => break,
                        Some(c) if c.is_ascii_hexdigit() => r.push(c),
                        _ => bail!("unrecognized token"),
                    }
                }
                let len = r.len() + 3;
                (Token::Bytes(r), len)
            }
            c if c.is_ascii_whitespace() => {
                let mut r = String::new();
                r.push(c);
                for c in it {
                    if c.is_ascii_whitespace() {
                        r.push(c);
                    } else {
                        break;
                    }
                }
                let len = r.len();
                (Token::Whitespace(r), len)
            }
            c if c.is_ascii_alphabetic() => {
                let mut r = String::new();
                r.push(c);
                for c in it {
                    if identifier::is_valid_identifier_char(c) {
                        r.push(c);
                    } else {
                        break;
                    }
                }
                let len = r.len();
                (name_token(r), len)
            }
            _ => bail!("unrecognized token"),
        })),
    }
}

fn tokenize(mut s: &str) -> Result<Vec<Token>> {
    let mut v = vec![];
    while let Some((tok, n)) = next_token(s)? {
        v.push(tok);
        s = &s[n..];
    }
    Ok(v)
}

struct Parser<I: Iterator<Item = Token>> {
    it: Peekable<I>,
}

impl<I: Iterator<Item = Token>> Parser<I> {
    fn new<T: IntoIterator<Item = Token, IntoIter = I>>(v: T) -> Self {
        Self {
            it: v.into_iter().peekable(),
        }
    }

    fn next(&mut self) -> Result<Token> {
        match self.it.next() {
            Some(tok) => Ok(tok),
            None => bail!("out of tokens, this should not happen"),
        }
    }

    fn peek(&mut self) -> Option<&Token> {
        self.it.peek()
    }

    fn consume(&mut self, tok: Token) -> Result<()> {
        let t = self.next()?;
        if t != tok {
            bail!("expected token {:?}, got {:?}", tok, t)
        }
        Ok(())
    }

    fn parse_comma_list<F, R>(
        &mut self,
        parse_list_item: F,
        end_token: Token,
        allow_trailing_comma: bool,
    ) -> Result<Vec<R>>
    where
        F: Fn(&mut Self) -> Result<R>,
        R: std::fmt::Debug,
    {
        let mut v = vec![];
        if !(self.peek() == Some(&end_token)) {
            loop {
                v.push(parse_list_item(self)?);
                if self.peek() == Some(&end_token) {
                    break;
                }
                self.consume(Token::Comma)?;
                if self.peek() == Some(&end_token) && allow_trailing_comma {
                    break;
                }
            }
        }
        Ok(v)
    }

    fn parse_type_tag(&mut self) -> Result<TypeTag> {
        Ok(match self.next()? {
            Token::U8Type => TypeTag::U8,
            Token::U16Type => TypeTag::U16,
            Token::U32Type => TypeTag::U32,
            Token::U64Type => TypeTag::U64,
            Token::U128Type => TypeTag::U128,
            Token::U256Type => TypeTag::U256,
            Token::BoolType => TypeTag::Bool,
            Token::AddressType => TypeTag::Address,
            Token::SignerType => TypeTag::Signer,
            Token::VectorType => {
                self.consume(Token::Lt)?;
                let ty = self.parse_type_tag()?;
                self.consume(Token::Gt)?;
                TypeTag::Vector(Box::new(ty))
            }
            Token::Address(addr) => {
                self.consume(Token::ColonColon)?;
                let module = token_to_name(self.next()?)?;

                self.consume(Token::ColonColon)?;
                let name = token_to_name(self.next()?)?;
                let ty_args = if self.peek() == Some(&Token::Lt) {
                    self.next()?;
                    let ty_args =
                        self.parse_comma_list(|parser| parser.parse_type_tag(), Token::Gt, true)?;
                    self.consume(Token::Gt)?;
                    ty_args
                } else {
                    vec![]
                };
                TypeTag::Struct(Box::new(StructTag {
                    address: AccountAddress::from_hex_literal(&addr)?,
                    module: Identifier::new(module)?,
                    name: Identifier::new(name)?,
                    type_params: ty_args,
                }))
            }
            tok => bail!("unexpected token {:?}, expected type tag", tok),
        })
    }
}

fn parse<F, T>(s: &str, f: F) -> Result<T>
where
    F: Fn(&mut Parser<std::vec::IntoIter<Token>>) -> Result<T>,
{
    let mut tokens: Vec<_> = tokenize(s)?
        .into_iter()
        .filter(|tok| !tok.is_whitespace())
        .collect();
    tokens.push(Token::Eof);
    let mut parser = Parser::new(tokens);
    let res = f(&mut parser)?;
    parser.consume(Token::Eof)?;
    Ok(res)
}

pub fn parse_struct_tag(s: &str) -> Result<StructTag> {
    let type_tag = parse(s, |parser| parser.parse_type_tag())
        .map_err(|e| format_err!("invalid struct tag: {}, {}", s, e))?;
    if let TypeTag::Struct(struct_tag) = type_tag {
        Ok(*struct_tag)
    } else {
        bail!("invalid struct tag: {}", s)
    }
}

#[cfg(test)]
mod parser_tests {
    use crate::type_tag_parser::{parse, parse_struct_tag};
    use anyhow::Result;
    use move_core_types::{language_storage::TypeTag, parser as MCP};

    fn parse_type_tag(s: &str) -> Result<TypeTag> {
        parse(s, |parser| parser.parse_type_tag())
    }

    #[test]
    fn test_type_tag() {
        for s in &[
            "u64",
            "bool",
            "vector<u8>",
            "vector<vector<u64>>",
            "vector<u16>",
            "vector<vector<u16>>",
            "vector<u32>",
            "vector<vector<u32>>",
            "vector<u128>",
            "vector<vector<u128>>",
            "vector<u256>",
            "vector<vector<u256>>",
            "signer",
            "0x1::M::S",
            "0x2::M::S_",
            "0x3::M_::S",
            "0x4::M_::S_",
            "0x00000000004::M::S",
            "0x1::M::S<u64>",
            "0x1::M::S<u16>",
            "0x1::M::S<u32>",
            "0x1::M::S<u256>",
            "0x1::M::S<0x2::P::Q>",
            "vector<0x1::M::S>",
            "vector<0x1::M_::S_>",
            "vector<vector<0x1::M_::S_>>",
            "0x1::M::S<vector<u8>>",
            "0x1::M::S<vector<u16>>",
            "0x1::M::S<vector<u32>>",
            "0x1::M::S<vector<u64>>",
            "0x1::M::S<vector<u128>>",
            "0x1::M::S<vector<u256>>",
        ] {
            let new = parse_type_tag(s);
            let old = MCP::parse_type_tag(s);
            assert!(new.is_ok(), "Failed to parse tag {}", s);
            assert!(old.is_ok(), "Failed to parse tag {}", s);
            assert_eq!(old.unwrap(), new.unwrap());
        }
    }

    #[test]
    fn test_parse_valid_struct_tag() {
        let valid = vec![
            "0x1::Diem::Diem",
            "0x1::Diem_Type::Diem",
            "0x1::Diem_::Diem",
            "0x1::X_123::X32_",
            "0x1::Diem::Diem_Type",
            "0x1::Diem::Diem<0x1::XDX::XDX>",
            "0x1::Diem::Diem<0x1::XDX::XDX_Type>",
            "0x1::Diem::Diem<u8>",
            "0x1::Diem::Diem<u64>",
            "0x1::Diem::Diem<u128>",
            "0x1::Diem::Diem<u16>",
            "0x1::Diem::Diem<u32>",
            "0x1::Diem::Diem<u256>",
            "0x1::Diem::Diem<bool>",
            "0x1::Diem::Diem<address>",
            "0x1::Diem::Diem<signer>",
            "0x1::Diem::Diem<vector<0x1::XDX::XDX>>",
            "0x1::Diem::Diem<u8,bool>",
            "0x1::Diem::Diem<u8,   bool>",
            "0x1::Diem::Diem<u16,bool>",
            "0x1::Diem::Diem<u32,   bool>",
            "0x1::Diem::Diem<u128,bool>",
            "0x1::Diem::Diem<u256,   bool>",
            "0x1::Diem::Diem<u8  ,bool>",
            "0x1::Diem::Diem<u8 , bool  ,    vector<u8>,address,signer>",
            "0x1::Diem::Diem<vector<0x1::Diem::Struct<0x1::XUS::XUS>>>",
            "0x1::Diem::Diem<0x1::Diem::Struct<vector<0x1::XUS::XUS>, 0x1::Diem::Diem<vector<0x1::Diem::Struct<0x1::XUS::XUS>>>>>",
            ];

        let new = vec![
            "0x1::address::MyType",
            "0x1::vector::MyType",
            "0x1::address::MyType<0x1::address::OtherType>",
            "0x1::address::MyType<0x1::address::OtherType, 0x1::vector::VecTyper>",
            "0x1::address::address<0x1::vector::address, 0x1::vector::vector>",
        ];
        for text in valid {
            let st = parse_struct_tag(text).expect("valid StructTag");
            let old_st = MCP::parse_struct_tag(text).expect("valid StructTag");
            assert_eq!(
                st.to_string().replace(' ', ""),
                text.replace(' ', ""),
                "text: {:?}, StructTag: {:?}",
                text,
                st
            );

            assert_eq!(st, old_st);
        }

        for text in new {
            let st = parse_struct_tag(text).expect("valid StructTag");
            assert_eq!(
                st.to_string().replace(' ', ""),
                text.replace(' ', ""),
                "text: {:?}, StructTag: {:?}",
                text,
                st
            );
        }
    }
}
