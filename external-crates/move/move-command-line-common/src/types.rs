// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Display};

use anyhow::bail;
use move_core_types::{
    account_address::AccountAddress,
    identifier::{self, Identifier},
    language_storage::{StructTag, TypeTag},
};

use crate::{address::ParsedAddress, parser::Token};

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum TypeToken {
    Whitespace,
    Ident,
    AddressIdent,
    ColonColon,
    Lt,
    Gt,
    Comma,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct ParsedStructType {
    pub address: ParsedAddress,
    pub module: String,
    pub name: String,
    pub type_args: Vec<ParsedType>,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum ParsedType {
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Bool,
    Address,
    Signer,
    Vector(Box<ParsedType>),
    Struct(ParsedStructType),
}

impl Display for TypeToken {
    fn fmt<'f>(&self, formatter: &mut fmt::Formatter<'f>) -> Result<(), fmt::Error> {
        let s = match *self {
            TypeToken::Whitespace => "[whitespace]",
            TypeToken::Ident => "[identifier]",
            TypeToken::AddressIdent => "[address]",
            TypeToken::ColonColon => "::",
            TypeToken::Lt => "<",
            TypeToken::Gt => ">",
            TypeToken::Comma => ",",
        };
        fmt::Display::fmt(s, formatter)
    }
}

impl Token for TypeToken {
    fn is_whitespace(&self) -> bool {
        matches!(self, Self::Whitespace)
    }

    fn next_token(s: &str) -> anyhow::Result<Option<(Self, usize)>> {
        let mut chars = s.chars().peekable();

        let c = match chars.next() {
            None => return Ok(None),
            Some(c) => c,
        };
        Ok(Some(match c {
            '<' => (Self::Lt, 1),
            '>' => (Self::Gt, 1),
            ',' => (Self::Comma, 1),
            ':' => match chars.next() {
                Some(':') => (Self::ColonColon, 2),
                _ => bail!("unrecognized token: {}", s),
            },
            '0' if matches!(chars.peek(), Some('x') | Some('X')) => {
                chars.next().unwrap();
                match chars.next() {
                    Some(c) if c.is_ascii_hexdigit() || c == '_' => {
                        // 0x + c + remaining
                        let len = 3 + chars
                            .take_while(|q| char::is_ascii_hexdigit(q) || *q == '_')
                            .count();
                        (Self::AddressIdent, len)
                    }
                    _ => bail!("unrecognized token: {}", s),
                }
            }
            c if c.is_ascii_digit() => {
                // c + remaining
                let len = 1 + chars.take_while(char::is_ascii_digit).count();
                (Self::AddressIdent, len)
            }
            c if c.is_ascii_whitespace() => {
                // c + remaining
                let len = 1 + chars.take_while(char::is_ascii_whitespace).count();
                (Self::Whitespace, len)
            }
            c if c.is_ascii_alphabetic() => {
                // c + remaining
                let len = 1 + chars
                    .take_while(|c| identifier::is_valid_identifier_char(*c))
                    .count();
                (Self::Ident, len)
            }
            _ => bail!("unrecognized token: {}", s),
        }))
    }
}

impl ParsedStructType {
    pub fn into_struct_tag(
        self,
        mapping: &impl Fn(&str) -> Option<AccountAddress>,
    ) -> anyhow::Result<StructTag> {
        let Self {
            address,
            module,
            name,
            type_args,
        } = self;
        Ok(StructTag {
            address: address.into_account_address(mapping)?,
            module: Identifier::new(module)?,
            name: Identifier::new(name)?,
            type_params: type_args
                .into_iter()
                .map(|t| t.into_type_tag(mapping))
                .collect::<anyhow::Result<_>>()?,
        })
    }
}

impl ParsedType {
    pub fn into_type_tag(
        self,
        mapping: &impl Fn(&str) -> Option<AccountAddress>,
    ) -> anyhow::Result<TypeTag> {
        Ok(match self {
            ParsedType::U8 => TypeTag::U8,
            ParsedType::U16 => TypeTag::U16,
            ParsedType::U32 => TypeTag::U32,
            ParsedType::U64 => TypeTag::U64,
            ParsedType::U128 => TypeTag::U128,
            ParsedType::U256 => TypeTag::U256,
            ParsedType::Bool => TypeTag::Bool,
            ParsedType::Address => TypeTag::Address,
            ParsedType::Signer => TypeTag::Signer,
            ParsedType::Vector(inner) => TypeTag::Vector(Box::new(inner.into_type_tag(mapping)?)),
            ParsedType::Struct(s) => TypeTag::Struct(Box::new(s.into_struct_tag(mapping)?)),
        })
    }
}
