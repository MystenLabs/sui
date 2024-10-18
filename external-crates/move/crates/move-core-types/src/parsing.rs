// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress,
    identifier::{self, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
    u256::U256,
};
use anyhow::{bail, Result};
use std::{
    fmt::{self, Display},
    iter::Peekable,
    str::FromStr,
};

//---------------------------------------------------------------------------
// Public APIs
//---------------------------------------------------------------------------

impl TypeTag {
    pub fn parse_with_address_resolver(
        s: &str,
        address_resolver: &dyn Fn(&str) -> Option<AccountAddress>,
    ) -> Result<Self> {
        let tokens = TypeToken::tokenize(s)?;
        let mut parser = TagParser::new(tokens, address_resolver);
        parser.parse_type_tag()
    }

    pub fn parse(s: &str) -> Result<Self> {
        Self::parse_with_address_resolver(s, &|_| None)
    }
}

impl StructTag {
    pub fn parse_with_address_resolver(
        s: &str,
        address_resolver: &dyn Fn(&str) -> Option<AccountAddress>,
    ) -> Result<Self> {
        let tokens = TypeToken::tokenize(s)?;
        let mut parser = TagParser::new(tokens, address_resolver);
        parser.parse_struct_tag()
    }

    pub fn parse(s: &str) -> Result<Self> {
        Self::parse_with_address_resolver(s, &|_| None)
    }
}

impl ModuleId {
    pub fn parse_with_address_resolver(
        s: &str,
        address_resolver: &dyn Fn(&str) -> Option<AccountAddress>,
    ) -> Result<Self> {
        let tokens = TypeToken::tokenize(s)?;
        let mut parser = TagParser::new(tokens, address_resolver);
        parser.parse_module_id()
    }

    pub fn parse(s: &str) -> Result<Self> {
        let tokens = TypeToken::tokenize(s)?;
        let mut parser = TagParser::new(tokens, &|_| None);
        parser.parse_module_id()
    }
}

impl FromStr for TypeTag {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl FromStr for StructTag {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl FromStr for ModuleId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

pub fn parse_fq_name(s: &str) -> Result<(ModuleId, Identifier)> {
    parse_fq_name_with_address_resolver(s, &|_| None)
}

pub fn parse_fq_name_with_address_resolver(
    s: &str,
    address_resolver: &dyn Fn(&str) -> Option<AccountAddress>,
) -> Result<(ModuleId, Identifier)> {
    let tokens = TypeToken::tokenize(s)?;
    let mut parser = TagParser::new(tokens, address_resolver);
    parser.parse_fq_name()
}

pub fn parse_address_with_resolver(
    s: &str,
    address_resolver: &dyn Fn(&str) -> Option<AccountAddress>,
) -> Result<AccountAddress> {
    let tokens = TypeToken::tokenize(s)?;
    let mut parser = TagParser::new(tokens, address_resolver);
    parser.parse_address()
}

//---------------------------------------------------------------------------
// Implementation -- all private
//---------------------------------------------------------------------------

const MAX_TYPE_DEPTH: u64 = 128;
const MAX_TYPE_NODE_COUNT: u64 = 256;

struct TagParser<'a, I: Iterator<Item = (TypeToken, &'a str)>> {
    count: u64,
    it: Peekable<I>,
    address_resolver: &'a dyn Fn(&str) -> Option<AccountAddress>,
}

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
enum TypeToken {
    Whitespace,
    Ident,
    AddressIdent,
    NumericalIdent,
    ColonColon,
    Lt,
    Gt,
    Comma,
}

impl Display for TypeToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let s = match *self {
            TypeToken::Whitespace => "[whitespace]",
            TypeToken::Ident => "[identifier]",
            TypeToken::AddressIdent => "[address]",
            TypeToken::NumericalIdent => "[numerical_address]",
            TypeToken::ColonColon => "::",
            TypeToken::Lt => "<",
            TypeToken::Gt => ">",
            TypeToken::Comma => ",",
        };
        fmt::Display::fmt(s, formatter)
    }
}

impl TypeToken {
    fn tokenize(mut s: &str) -> Result<Vec<(Self, &str)>> {
        let mut v = vec![];
        while let Some((tok, n)) = Self::next_token(s)? {
            if tok != TypeToken::Whitespace {
                v.push((tok, &s[..n]));
            }
            s = &s[n..];
        }
        Ok(v)
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
                chars.next().expect("Just peeked so exists.");
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
                (Self::NumericalIdent, len)
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

impl<'a, I: Iterator<Item = (TypeToken, &'a str)>> TagParser<'a, I> {
    fn new<T: IntoIterator<Item = (TypeToken, &'a str), IntoIter = I>>(
        v: T,
        address_resolver: &'a dyn Fn(&str) -> Option<AccountAddress>,
    ) -> Self {
        Self {
            count: 0,
            it: v.into_iter().peekable(),
            address_resolver,
        }
    }

    fn parse_type_tag(&mut self) -> Result<TypeTag> {
        self.parse_type_impl(0)
    }

    fn parse_struct_tag(&mut self) -> Result<StructTag> {
        match self.parse_type_tag()? {
            TypeTag::Struct(struct_tag) => Ok(*struct_tag),
            tag => bail!("expected struct tag, got {}", tag),
        }
    }

    fn parse_module_id(&mut self) -> Result<ModuleId> {
        let id = match self.advance_any()? {
            (
                tok @ (TypeToken::Ident | TypeToken::AddressIdent | TypeToken::NumericalIdent),
                contents,
            ) => self.parse_module_id_impl(tok, contents),
            (tok, _) => bail!("unexpected token {tok}, expected address"),
        }?;
        if let Ok((tok, _)) = self.advance_any() {
            bail!("unexpected token {tok}, expected end of input")
        }
        Ok(id)
    }

    fn parse_fq_name(&mut self) -> Result<(ModuleId, Identifier)> {
        let fq_name = match self.advance_any()? {
            (
                tok @ (TypeToken::Ident | TypeToken::AddressIdent | TypeToken::NumericalIdent),
                contents,
            ) => self.parse_fq_name_impl(tok, contents),
            (tok, _) => bail!("unexpected token {tok}, expected address"),
        }?;
        if let Ok((tok, _)) = self.advance_any() {
            bail!("unexpected token {tok}, expected end of input")
        }
        Ok(fq_name)
    }

    fn parse_address(&mut self) -> Result<AccountAddress> {
        let addr = match self.advance_any()? {
            (
                tok @ (TypeToken::Ident | TypeToken::AddressIdent | TypeToken::NumericalIdent),
                contents,
            ) => self.parse_address_impl(tok, contents),
            (tok, _) => bail!("unexpected token {tok}, expected address"),
        }?;
        if let Ok((tok, _)) = self.advance_any() {
            bail!("unexpected token {tok}, expected end of input")
        }
        Ok(addr)
    }

    fn parse_address_impl(&mut self, tok: TypeToken, contents: &'a str) -> Result<AccountAddress> {
        Ok(match tok {
            TypeToken::Ident => {
                let address = (self.address_resolver)(contents);
                if let Some(address) = address {
                    address
                } else {
                    bail!("Unbound named address: '{contents}'");
                }
            }
            TypeToken::AddressIdent => AccountAddress::from_str(contents)
                .map_err(|_| anyhow::anyhow!("invalid address"))?,
            TypeToken::NumericalIdent => AccountAddress::new(
                U256::from_str(contents)
                    .map_err(|_| anyhow::anyhow!("invalid address"))?
                    .to_le_bytes(),
            ),
            tok => bail!("unexpected token {tok}, expected address"),
        })
    }

    fn parse_module_id_impl(&mut self, tok: TypeToken, contents: &'a str) -> Result<ModuleId> {
        let address = self.parse_address_impl(tok, contents)?;
        self.advance(TypeToken::ColonColon)?;
        let name = self.advance(TypeToken::Ident)?.to_owned();
        Ok(ModuleId::new(address, Identifier::new(name)?))
    }

    fn parse_fq_name_impl(
        &mut self,
        tok: TypeToken,
        contents: &'a str,
    ) -> Result<(ModuleId, Identifier)> {
        let module = self.parse_module_id_impl(tok, contents)?;
        self.advance(TypeToken::ColonColon)?;
        let name = self.advance(TypeToken::Ident)?.to_owned();
        Ok((module, Identifier::new(name)?))
    }

    fn parse_type_impl(&mut self, depth: u64) -> Result<TypeTag> {
        self.count += 1;

        if depth > MAX_TYPE_DEPTH || self.count > MAX_TYPE_NODE_COUNT {
            bail!("Type exceeds maximum nesting depth or node count")
        }

        Ok(match self.advance_any()? {
            (TypeToken::Ident, "u8") => TypeTag::U8,
            (TypeToken::Ident, "u16") => TypeTag::U16,
            (TypeToken::Ident, "u32") => TypeTag::U32,
            (TypeToken::Ident, "u64") => TypeTag::U64,
            (TypeToken::Ident, "u128") => TypeTag::U128,
            (TypeToken::Ident, "u256") => TypeTag::U256,
            (TypeToken::Ident, "bool") => TypeTag::Bool,
            (TypeToken::Ident, "address") => TypeTag::Address,
            (TypeToken::Ident, "signer") => TypeTag::Signer,
            (TypeToken::Ident, "vector") => {
                self.advance(TypeToken::Lt)?;
                let ty = self.parse_type_impl(depth + 1)?;
                self.advance(TypeToken::Gt)?;
                TypeTag::Vector(Box::new(ty))
            }

            (
                tok @ (TypeToken::Ident | TypeToken::AddressIdent | TypeToken::NumericalIdent),
                contents,
            ) => {
                let (mid, name) = self.parse_fq_name_impl(tok, contents)?;
                let (address, module) = mid.into();
                let type_params = match self.peek_tok() {
                    Some(TypeToken::Lt) => {
                        self.advance(TypeToken::Lt)?;
                        let type_args = self.parse_list(
                            |parser| parser.parse_type_impl(depth + 1),
                            TypeToken::Comma,
                            TypeToken::Gt,
                            true,
                        )?;
                        self.advance(TypeToken::Gt)?;
                        type_args
                    }
                    _ => vec![],
                };
                TypeTag::Struct(Box::new(StructTag {
                    address,
                    module,
                    name,
                    type_params,
                }))
            }
            (tok, _) => bail!("unexpected token {tok}, expected type"),
        })
    }

    fn advance_any(&mut self) -> Result<(TypeToken, &'a str)> {
        match self.it.next() {
            Some(tok) => Ok(tok),
            None => bail!("unexpected end of tokens"),
        }
    }

    fn advance(&mut self, expected_token: TypeToken) -> Result<&'a str> {
        let (t, contents) = self.advance_any()?;
        if t != expected_token {
            bail!("expected token {}, got {}", expected_token, t)
        }
        Ok(contents)
    }

    fn peek_tok(&mut self) -> Option<TypeToken> {
        self.it.peek().map(|(tok, _)| *tok)
    }

    fn parse_list<R>(
        &mut self,
        parse_list_item: impl Fn(&mut Self) -> Result<R>,
        delim: TypeToken,
        end_token: TypeToken,
        allow_trailing_delim: bool,
    ) -> Result<Vec<R>> {
        let is_end = |tok_opt: Option<TypeToken>| -> bool {
            tok_opt.map(|tok| tok == end_token).unwrap_or(true)
        };
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::language_storage::{ModuleId, StructTag, TypeTag};
    use crate::parsing::{parse_address_with_resolver, parse_fq_name};
    use crate::u256::U256;
    use crate::{account_address::AccountAddress, identifier::Identifier};
    use proptest::prelude::*;
    use proptest::proptest;

    #[test]
    fn test_parse_type_negative() {
        for s in &[
            "_",
            "_::_::_",
            "0x1::_",
            "0x1::__::_",
            "0x1::_::__",
            "0x1::_::foo",
            "0x1::foo::_",
            "0x1::_::_",
            "0x1::bar::foo<0x1::_::foo>",
            "0X1::bar::bar",
        ] {
            assert!(
                TypeTag::parse(s).is_err(),
                "Parsed type {s} but should have failed"
            );
        }
    }

    #[test]
    fn test_parse_struct_negative() {
        for s in &[
            "_",
            "_::_::_",
            "0x1::_",
            "0x1::__::_",
            "0x1::_::__",
            "0x1::_::foo",
            "0x1::foo::_",
            "0x1::_::_",
            "0x1::bar::foo<0x1::_::foo>",
        ] {
            assert!(
                TypeTag::parse(s).is_err(),
                "Parsed type {s} but should have failed"
            );
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
            "0x1::_bar::_BAR",
            "0x1::__::__",
            "0x1::_bar::_BAR<0x2::_____::______fooo______>",
            "0x1::__::__<0x2::_____::______fooo______, 0xff::Bar____::_______foo>",
        ] {
            assert!(TypeTag::parse(s).is_ok(), "Failed to parse type {}", s);
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
            "0x1::_bar::_BAR",
            "0x1::__::__",
            "0x1::_bar::_BAR<0x2::_____::______fooo______>",
            "0x1::__::__<0x2::_____::______fooo______, 0xff::Bar____::_______foo>",
        ];
        for s in valid {
            assert!(StructTag::parse(s).is_ok(), "Failed to parse struct {}", s);
        }
    }

    fn struct_type_gen0() -> impl Strategy<Value = String> {
        (
            any::<AccountAddress>(),
            any::<Identifier>(),
            any::<Identifier>(),
        )
            .prop_map(|(address, module, name)| format!("0x{}::{}::{}", address, module, name))
    }

    fn struct_type_gen1() -> impl Strategy<Value = String> {
        (any::<U256>(), any::<Identifier>(), any::<Identifier>())
            .prop_map(|(address, module, name)| format!("{}::{}::{}", address, module, name))
    }

    fn module_id_gen0() -> impl Strategy<Value = String> {
        (any::<AccountAddress>(), any::<Identifier>())
            .prop_map(|(address, module)| format!("0x{address}::{module}"))
    }

    fn module_id_gen1() -> impl Strategy<Value = String> {
        (any::<U256>(), any::<Identifier>())
            .prop_map(|(address, module)| format!("{address}::{module}"))
    }

    fn fq_id_gen0() -> impl Strategy<Value = String> {
        (
            any::<AccountAddress>(),
            any::<Identifier>(),
            any::<Identifier>(),
        )
            .prop_map(|(address, module, name)| format!("0x{address}::{module}::{name}"))
    }

    fn fq_id_gen1() -> impl Strategy<Value = String> {
        (any::<U256>(), any::<Identifier>(), any::<Identifier>())
            .prop_map(|(address, module, name)| format!("{address}::{module}::{name}"))
    }

    proptest! {
        #[test]
        fn test_parse_valid_struct_type_proptest0(s in struct_type_gen0(), x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(StructTag::from_str(&s).is_ok());
            prop_assert!(TypeTag::from_str(&s).is_ok());
            prop_assert!(parse_fq_name(&s).is_ok());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());

            // Add remainder string
            let s = s + &x;
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());

        }

        #[test]
        fn test_parse_valid_struct_type_proptest1(s in struct_type_gen1(), x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(StructTag::from_str(&s).is_ok());
            prop_assert!(TypeTag::from_str(&s).is_ok());
            prop_assert!(parse_fq_name(&s).is_ok());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            // add remainder string
            let s = s + &x;
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
        }

        #[test]
        fn test_parse_valid_module_id_proptest0(s in module_id_gen0(), x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(ModuleId::from_str(&s).is_ok());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            // add remainder string
            let s = s + &x;
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
        }

        #[test]
        fn test_parse_valid_module_id_proptest1(s in module_id_gen1(), x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(ModuleId::from_str(&s).is_ok());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            // add remainder String
            let s = s + &x;
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());

        }

        #[test]
        fn test_parse_valid_fq_id_proptest0(s in fq_id_gen0(), x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(parse_fq_name(&s).is_ok());
            prop_assert!(StructTag::from_str(&s).is_ok());
            prop_assert!(TypeTag::from_str(&s).is_ok());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            // add remainder string
            let s = s + &x;
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
        }

        #[test]
        fn test_parse_valid_fq_id_proptest1(s in fq_id_gen1(), x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(parse_fq_name(&s).is_ok());
            prop_assert!(StructTag::from_str(&s).is_ok());
            prop_assert!(TypeTag::from_str(&s).is_ok());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            let s = s + &x;
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
        }

        #[test]
        fn test_parse_valid_numeric_address(s in "[0-9]{64}", x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(AccountAddress::from_str(&s).is_ok());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_ok());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            // add remainder string
            let s = s + &x;
            prop_assert!(AccountAddress::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
        }

        #[test]
        fn test_parse_different_length_numeric_addresses(s in "[0-9]{1,63}", x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(AccountAddress::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_ok());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            // add remainder string
            let s = s + &x;
            prop_assert!(AccountAddress::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
        }

        #[test]
        fn test_parse_valid_hex_address(s in "0x[0-9a-fA-F]{64}", x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(AccountAddress::from_str(&s).is_ok());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_ok());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            // add remainder string
            let s = s + &x;
            prop_assert!(AccountAddress::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
        }

        #[test]
        fn test_parse_invalid_hex_address(s in "[0-9]{63}[a-fA-F]{1}", x in r#"[^a-zA-Z0-9_\s]+"#) {
            prop_assert!(AccountAddress::from_str(&s).is_ok());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
            // add remainder string
            let s = s + &x;
            prop_assert!(AccountAddress::from_str(&s).is_err());
            prop_assert!(parse_address_with_resolver(&s, &|_| None).is_err());
            prop_assert!(parse_fq_name(&s).is_err());
            prop_assert!(ModuleId::from_str(&s).is_err());
            prop_assert!(StructTag::from_str(&s).is_err());
            prop_assert!(TypeTag::from_str(&s).is_err());
        }
    }
}
