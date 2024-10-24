// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Display};

use crate::{
    account_address::AccountAddress,
    identifier::{self, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
};
use anyhow::bail;

use crate::parsing::{address::ParsedAddress, parser::Token};

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
pub struct ParsedModuleId {
    pub address: ParsedAddress,
    pub name: String,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct ParsedFqName {
    pub module: ParsedModuleId,
    pub name: String,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct ParsedStructType {
    pub fq_name: ParsedFqName,
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
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
            '0' if matches!(chars.peek(), Some('x')) => {
                chars.next().unwrap();
                match chars.next() {
                    Some(c) if c.is_ascii_hexdigit() => {
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
                let len = 1 + chars
                    .take_while(|c| c.is_ascii_digit() || *c == '_')
                    .count();
                (Self::AddressIdent, len)
            }
            c if c.is_ascii_whitespace() => {
                // c + remaining
                let len = 1 + chars.take_while(char::is_ascii_whitespace).count();
                (Self::Whitespace, len)
            }
            c if c.is_ascii_alphabetic()
                || (c == '_'
                    && chars
                        .peek()
                        .is_some_and(|c| identifier::is_valid_identifier_char(*c))) =>
            {
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

impl ParsedModuleId {
    pub fn into_module_id(
        self,
        mapping: &impl Fn(&str) -> Option<AccountAddress>,
    ) -> anyhow::Result<ModuleId> {
        Ok(ModuleId::new(
            self.address.into_account_address(mapping)?,
            Identifier::new(self.name)?,
        ))
    }
}

impl ParsedFqName {
    pub fn into_fq_name(
        self,
        mapping: &impl Fn(&str) -> Option<AccountAddress>,
    ) -> anyhow::Result<(ModuleId, String)> {
        Ok((self.module.into_module_id(mapping)?, self.name))
    }
}

impl ParsedStructType {
    pub fn into_struct_tag(
        self,
        mapping: &impl Fn(&str) -> Option<AccountAddress>,
    ) -> anyhow::Result<StructTag> {
        let Self { fq_name, type_args } = self;
        Ok(StructTag {
            address: fq_name.module.address.into_account_address(mapping)?,
            module: Identifier::new(fq_name.module.name)?,
            name: Identifier::new(fq_name.name)?,
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
