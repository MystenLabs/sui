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
        TypeToken::tokenize(s)
            .and_then(|tokens| {
                let mut parser = TagParser::new(tokens, address_resolver);
                let tag = parser.parse_type_tag()?;
                parser.expect_end()?;
                Ok(tag)
            })
            .map_err(|e| anyhow::anyhow!("Invalid type tag '{s}': {e}"))
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
        TypeToken::tokenize(s)
            .and_then(|tokens| {
                let mut parser = TagParser::new(tokens, address_resolver);
                let tag = parser.parse_struct_tag()?;
                parser.expect_end()?;
                Ok(tag)
            })
            .map_err(|e| anyhow::anyhow!("Invalid struct type '{s}': {e}"))
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
        TypeToken::tokenize(s)
            .and_then(|tokens| {
                let mut parser = TagParser::new(tokens, address_resolver);
                let mid = parser.parse_module_id()?;
                parser.expect_end()?;
                Ok(mid)
            })
            .map_err(|e| anyhow::anyhow!("Invalid module ID '{s}': {e}"))
    }

    pub fn parse(s: &str) -> Result<Self> {
        Self::parse_with_address_resolver(s, &|_| None)
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
    TypeToken::tokenize(s)
        .and_then(|tokens| {
            let mut parser = TagParser::new(tokens, address_resolver);
            let fqn = parser.parse_fq_name()?;
            parser.expect_end()?;
            Ok(fqn)
        })
        .map_err(|e| anyhow::anyhow!("Invalid fully qualified name '{s}': {e}"))
}

pub fn parse_address_with_resolver(
    s: &str,
    address_resolver: &dyn Fn(&str) -> Option<AccountAddress>,
) -> Result<AccountAddress> {
    TypeToken::tokenize(s)
        .and_then(|tokens| {
            let mut parser = TagParser::new(tokens, address_resolver);
            let addr = parser.parse_address()?;
            parser.expect_end()?;
            Ok(addr)
        })
        .map_err(|e| anyhow::anyhow!("Invalid address '{s}': {e}"))
}

pub fn parse_type_tags_with_resolver(
    s: &str,
    start: Option<&str>,
    delim: &str,
    end: &str,
    allow_trailing_delim: bool,
    resolver: &dyn Fn(&str) -> Option<AccountAddress>,
) -> Result<Vec<TypeTag>> {
    let mut parser = TagParser::new(TypeToken::tokenize(s)?, resolver);
    let list = parser.parse_list(
        |parser| parser.parse_type_tag(),
        start,
        delim,
        end,
        allow_trailing_delim,
    )?;
    parser.expect_end()?;
    Ok(list)
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

    fn expect_end(&mut self) -> Result<()> {
        if let Ok((tok, _)) = self.advance_any() {
            bail!("unexpected token '{tok}', expected end of input")
        }
        Ok(())
    }

    fn parse_type_tag(&mut self) -> Result<TypeTag> {
        self.parse_type_impl(0)
    }

    fn parse_struct_tag(&mut self) -> Result<StructTag> {
        match self.parse_type_tag()? {
            TypeTag::Struct(struct_tag) => Ok(*struct_tag),
            tag => bail!("expected struct tag, got '{tag}'"),
        }
    }

    fn parse_module_id(&mut self) -> Result<ModuleId> {
        match self.advance_any()? {
            (
                tok @ (TypeToken::Ident | TypeToken::AddressIdent | TypeToken::NumericalIdent),
                contents,
            ) => self.parse_module_id_impl(tok, contents),
            (tok, _) => bail!("unexpected token '{tok}', expected address"),
        }
    }

    fn parse_fq_name(&mut self) -> Result<(ModuleId, Identifier)> {
        match self.advance_any()? {
            (
                tok @ (TypeToken::Ident | TypeToken::AddressIdent | TypeToken::NumericalIdent),
                contents,
            ) => self.parse_fq_name_impl(tok, contents),
            (tok, _) => bail!("unexpected token '{tok}', expected address"),
        }
    }

    fn parse_address(&mut self) -> Result<AccountAddress> {
        match self.advance_any()? {
            (
                tok @ (TypeToken::Ident | TypeToken::AddressIdent | TypeToken::NumericalIdent),
                contents,
            ) => self.parse_address_impl(tok, contents),
            (tok, _) => bail!("unexpected token '{tok}', expected address"),
        }
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
            tok => bail!("unexpected token '{tok}', expected address"),
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
                let type_params = match self.peek_tok_contents() {
                    Some("<") => self.parse_list(
                        |parser| parser.parse_type_impl(depth + 1),
                        Some("<"),
                        ",",
                        ">",
                        true,
                    )?,
                    _ => vec![],
                };
                TypeTag::Struct(Box::new(StructTag {
                    address,
                    module,
                    name,
                    type_params,
                }))
            }
            (tok, _) => bail!("unexpected token '{tok}', expected type"),
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

    fn peek_tok_contents(&mut self) -> Option<&str> {
        self.it.peek().map(|(_, contents)| *contents)
    }

    fn parse_list<R>(
        &mut self,
        parse_list_item: impl Fn(&mut Self) -> Result<R>,
        start: Option<&str>,
        delim: &str,
        end_token: &str,
        allow_trailing_delim: bool,
    ) -> Result<Vec<R>> {
        let is_end =
            |tok_opt: Option<&str>| -> bool { tok_opt.map(|tok| tok == end_token).unwrap_or(true) };
        let mut v = vec![];

        if let Some(start) = start {
            let (_, start_contents) = self.advance_any()?;
            if start_contents != start {
                bail!(
                    "Invalid type list: expected start token '{}', got '{}'",
                    start,
                    start_contents
                )
            }
        }

        while !is_end(self.peek_tok_contents()) {
            v.push(parse_list_item(self)?);
            if is_end(self.peek_tok_contents()) {
                break;
            }
            let (_, delim_contents) = self.advance_any()?;

            if delim_contents != delim {
                bail!(
                    "Invalid type list: expected delimiter '{}', got '{}'",
                    delim,
                    delim_contents
                )
            }

            if is_end(self.peek_tok_contents()) {
                if allow_trailing_delim {
                    break;
                } else {
                    bail!("Invalid type list: trailing delimiter '{delim}'")
                }
            }
        }

        let (_, end_contents) = self.advance_any()?;
        if end_contents != end_token {
            bail!(
                "Invalid type list: expected end token '{}', got '{}'",
                end_token,
                end_contents
            )
        }

        if v.is_empty() {
            bail!("Invalid type list: empty list")
        }

        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        account_address::AccountAddress,
        identifier::Identifier,
        language_storage::{ModuleId, StructTag, TypeTag},
        parsing::{parse_address_with_resolver, parse_fq_name, parse_type_tags_with_resolver},
        u256::U256,
    };
    use proptest::{prelude::*, proptest};
    use std::str::FromStr;

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
            "0x1::bar::bar::foo",
            "0x1::Foo::Foo<",
            "0x1::Foo::Foo<0x1::ABC::ABC",
            "0x1::Foo::Foo<0x1::ABC::ABC::>",
            "0x1::Foo::Foo<0x1::ABC::ABC::A>",
            "0x1::Foo::Foo<>",
            "0x1::Foo::Foo<,>",
            "0x1::Foo::Foo<,",
            "0x1::Foo::Foo,>",
            "0x1::Foo::Foo>",
            "0x1::Foo::Foo,",
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

    #[test]
    fn test_parse_type_list() {
        let valid_with_trails = vec![
            "<u64,>",
            "<u64, 0x0::a::a,>",
            "<u64, 0x0::a::a, 0x0::a::a<0x0::a::a>,>",
        ];
        let valid_no_trails = vec![
            "<u64>",
            "<u64, 0x0::a::a>",
            "<u64, 0x0::a::a, 0x0::a::a<0x0::a::a>>",
        ];
        let invalid = vec![
            "<>",
            "<,>",
            "<u64,,>",
            "<,u64>",
            "<,u64,>",
            ",",
            "",
            "<",
            "<<",
            "><",
            ">,<",
            ">,",
            ",>",
            ",,",
            ">>",
            "<u64, u64",
            "<u64, u64,",
            "u64>",
            "u64,>",
            "u64, u64,>",
            "u64, u64,",
            "u64, u64",
            "u64 u64",
            "<u64 u64>",
            "<u64 u64,>",
            "u64 u64,",
            "<u64, 0x0::a::a, 0x0::a::a<0x::a::a>",
            "<u64, 0x0::a::a, 0x0::a::a<0x0::a::a>,",
            "<u64, 0x0::a::a, 0x0::a::a<0x0::a::a>,,>",
        ];

        for t in valid_no_trails.iter().chain(valid_with_trails.iter()) {
            assert!(parse_type_tags_with_resolver(t, Some("<"), ",", ">", true, &|_| None).is_ok());
        }

        for t in &valid_no_trails {
            assert!(
                parse_type_tags_with_resolver(t, Some("<"), ",", ">", false, &|_| None).is_ok()
            );
        }

        for t in &valid_with_trails {
            assert!(
                parse_type_tags_with_resolver(t, Some("<"), ",", ">", false, &|_| None).is_err()
            );
        }

        for t in &invalid {
            assert!(
                parse_type_tags_with_resolver(t, Some("<"), ",", ">", true, &|_| None).is_err()
            );
            assert!(
                parse_type_tags_with_resolver(t, Some("<"), ",", ">", false, &|_| None).is_err()
            );
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
        fn parse_type_tag_list(t in struct_type_gen0(), args in proptest::collection::vec(struct_type_gen0(), 1..=100)) {
            let s_no_trail = format!("<{}>", args.join(","));
            let s_with_trail = format!("<{},>", args.join(","));
            let s_no_trail_no_trail = parse_type_tags_with_resolver(&s_no_trail, Some("<"), ",", ">", false, &|_| None);
            let s_no_trail_allow_trail = parse_type_tags_with_resolver(&s_no_trail, Some("<"), ",", ">", true, &|_| None);
            let s_with_trail_no_trail = parse_type_tags_with_resolver(&s_with_trail, Some("<"), ",", ">", false, &|_| None);
            let s_with_trail_allow_trail = parse_type_tags_with_resolver(&s_with_trail, Some("<"), ",", ">", true, &|_| None);
            prop_assert!(s_no_trail_no_trail.is_ok());
            prop_assert!(s_no_trail_allow_trail.is_ok());
            prop_assert!(s_with_trail_no_trail.is_err());
            prop_assert!(s_with_trail_allow_trail.is_ok());
            let t_with_trail = format!("{t}{s_no_trail}");
            let t_no_trail = format!("{t}{s_with_trail}");
            let t_with_trail = TypeTag::parse(&t_with_trail);
            let t_no_trail = TypeTag::parse(&t_no_trail);
            prop_assert!(t_with_trail.is_ok());
            prop_assert!(t_no_trail.is_ok());
            prop_assert_eq!(t_with_trail.unwrap(), t_no_trail.unwrap());
        }

        #[test]
        fn test_parse_valid_struct_type_proptest0(s in struct_type_gen0(), x in r#"(::foo)[^a-zA-Z0-9_\s]+"#) {
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
        fn test_parse_valid_struct_type_proptest1(s in struct_type_gen1(), x in r#"(::foo)[^a-zA-Z0-9_\s]+"#) {
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
