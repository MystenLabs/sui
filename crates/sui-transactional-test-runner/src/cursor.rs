// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use bcs;
use bincode::{
    config::{BigEndian, Fixint},
    serde::BorrowCompat,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    u256::U256,
};
use serde::{
    ser::{SerializeSeq, SerializeTuple},
    Serialize,
};
use sui_types::base_types::ObjectID;
use winnow::{
    ascii::{hex_digit1, multispace0},
    combinator::{alt, opt, preceded, repeat, separated, seq},
    error::{ContextError as CE, FromExternalError, Result},
    token::{none_of, one_of, take_while},
    Parser,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    // Scalars
    Bytes(Vec<u8>),
    ObjectId(ObjectID),
    String(String),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(U256),

    // Types
    Struct(StructTag),

    // Aggregates
    List(Vec<Value>),
    Tuple(Vec<Value>),
}

enum Encoding {
    Bcs,
    Bin,
}

enum Suffix {
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Value::Bytes(bs) => bs.serialize(serializer),
            Value::ObjectId(id) => id.serialize(serializer),
            Value::String(s) => s.serialize(serializer),
            Value::U8(n) => n.serialize(serializer),
            Value::U16(n) => n.serialize(serializer),
            Value::U32(n) => n.serialize(serializer),
            Value::U64(n) => n.serialize(serializer),
            Value::U128(n) => n.serialize(serializer),
            Value::U256(n) => n.serialize(serializer),

            Value::Struct(tag) => tag.serialize(serializer),

            Value::List(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    seq.serialize_element(value)?;
                }
                seq.end()
            }

            Value::Tuple(values) => {
                let mut tuple = serializer.serialize_tuple(values.len())?;
                for value in values {
                    tuple.serialize_element(value)?;
                }
                tuple.end()
            }
        }
    }
}

/// Parse a string representing an encoded cursor. Cursors can either be BCS-encoded or bincoded,
/// and can contain:
///
/// - Object IDs (e.g., `0x1234`)
/// - Numbers (e.g., `42`, `1000u16`, `255u8`)
/// - Strings (e.g., `'hello world'`, `'it\'s working')
/// - Struct tags (e.g., `0x2::coin::Coin<0x2::sui::SUI>>`)
/// - Nested tuples (e.g., `(42, 'hello')`)
/// - Nested lists (e.g., `[1, 2, 3]`, `[]`)
/// - Nested encoded bytes (e.g., `bcs(0x1234)` or `bin(0x1234)`)
pub fn parse(mut input: &str) -> Result<Vec<u8>> {
    encoded(&mut input)
}

fn bytes(input: &mut &str) -> Result<Value> {
    Ok(Value::Bytes(encoded(input)?))
}

fn encoded(input: &mut &str) -> Result<Vec<u8>> {
    let enc = encode(input)?;
    let val = value(input)?;

    Ok(match enc {
        Encoding::Bcs => bcs::to_bytes(&val).map_err(|e| CE::from_external_error(input, e))?,
        Encoding::Bin => bincode::encode_to_vec(BorrowCompat(&val), bincode_config())
            .map_err(|e| CE::from_external_error(input, e))?,
    })
}

fn bincode_config() -> bincode::config::Configuration<BigEndian, Fixint> {
    bincode::config::standard()
        .with_big_endian()
        .with_fixed_int_encoding()
}

fn encode(input: &mut &str) -> Result<Encoding> {
    let ident = alt(("bin".map(|_| Encoding::Bin), "bcs".map(|_| Encoding::Bcs)));
    preceded(multispace0, ident).parse_next(input)
}

fn value(input: &mut &str) -> Result<Value> {
    // Consume preceding whitespace and then try to parse each variant of value in turn until one
    // succeeds. Ordering is relevant when handling grammars that share common prefixes (for
    // example, `struct_` and `id` both start with an address).
    preceded(
        multispace0,
        alt((struct_, id, number, string, bytes, tuple, list)),
    )
    .parse_next(input)
}

fn struct_(input: &mut &str) -> Result<Value> {
    struct_tag(input).map(Value::Struct)
}

fn struct_tag(input: &mut &str) -> Result<StructTag> {
    let (address, _, module, _, name, type_params) = seq!(
        address,
        "::",
        identifier,
        "::",
        identifier,
        opt(type_params)
    )
    .parse_next(input)?;

    Ok(StructTag {
        address,
        module,
        name,
        type_params: type_params.unwrap_or_default(),
    })
}

fn identifier(input: &mut &str) -> Result<Identifier> {
    let content = seq!((
        one_of(('a'..='z', 'A'..='Z', '_')),
        repeat::<_, _, String, _, _>(0.., one_of(('a'..='z', 'A'..='Z', '0'..='9', '_')))
    ))
    .take()
    .parse_next(input)?;

    Identifier::from_str(content).map_err(|_| CE::new())
}

fn type_params(input: &mut &str) -> Result<Vec<TypeTag>> {
    Ok(seq!(
        "<",
        separated(
            0..,
            preceded(multispace0, type_tag),
            preceded(multispace0, ",")
        ),
        preceded(multispace0, ">")
    )
    .parse_next(input)?
    .1)
}

fn type_tag(input: &mut &str) -> Result<TypeTag> {
    alt((
        "bool".map(|_| TypeTag::Bool),
        "u8".map(|_| TypeTag::U8),
        "u16".map(|_| TypeTag::U16),
        "u32".map(|_| TypeTag::U32),
        "u64".map(|_| TypeTag::U64),
        "u128".map(|_| TypeTag::U128),
        "u256".map(|_| TypeTag::U256),
        "address".map(|_| TypeTag::Address),
        vector,
        struct_tag.map(|s| TypeTag::Struct(Box::new(s))),
    ))
    .parse_next(input)
}

fn vector(input: &mut &str) -> Result<TypeTag> {
    let (_, _, inner, _) = seq!(
        "vector",
        preceded(multispace0, "<"),
        preceded(multispace0, type_tag),
        preceded(multispace0, ">")
    )
    .parse_next(input)?;
    Ok(TypeTag::Vector(Box::new(inner)))
}

fn id(input: &mut &str) -> Result<Value> {
    Ok(Value::ObjectId(address(input).map(Into::into)?))
}

fn address(input: &mut &str) -> Result<AccountAddress> {
    let addr = ("0x", hex_digit1).take().parse_next(input)?;
    AccountAddress::from_str(addr).map_err(|e| CE::from_external_error(input, e))
}

fn number(input: &mut &str) -> Result<Value> {
    let num = take_while(1.., |c: char| c.is_ascii_digit()).parse_next(input)?;
    let suffix = opt(suffix).parse_next(input)?.unwrap_or(Suffix::U64);

    Ok(match suffix {
        Suffix::U8 => Value::U8(num.parse().map_err(|e| CE::from_external_error(input, e))?),
        Suffix::U16 => Value::U16(num.parse().map_err(|e| CE::from_external_error(input, e))?),
        Suffix::U32 => Value::U32(num.parse().map_err(|e| CE::from_external_error(input, e))?),
        Suffix::U64 => Value::U64(num.parse().map_err(|e| CE::from_external_error(input, e))?),
        Suffix::U128 => Value::U128(num.parse().map_err(|e| CE::from_external_error(input, e))?),
        Suffix::U256 => Value::U256(num.parse().map_err(|e| CE::from_external_error(input, e))?),
    })
}

fn suffix(input: &mut &str) -> Result<Suffix> {
    alt((
        "u8".map(|_| Suffix::U8),
        "u16".map(|_| Suffix::U16),
        "u32".map(|_| Suffix::U32),
        "u64".map(|_| Suffix::U64),
        "u128".map(|_| Suffix::U128),
        "u256".map(|_| Suffix::U256),
    ))
    .parse_next(input)
}

fn string(input: &mut &str) -> Result<Value> {
    let (_, content, _) = seq!(
        "'",
        repeat(
            0..,
            alt(("\\'".map(|_| '\''), "\\\\".map(|_| '\\'), none_of('\'')))
        ),
        "'"
    )
    .parse_next(input)?;

    Ok(Value::String(content))
}

fn tuple(input: &mut &str) -> Result<Value> {
    let (_, values, _) = seq!(
        preceded(multispace0, "("),
        separated(1.., value, preceded(multispace0, ",")),
        preceded(multispace0, ")"),
    )
    .parse_next(input)?;

    Ok(Value::Tuple(values))
}

fn list(input: &mut &str) -> Result<Value> {
    let (_, values, _) = seq!(
        preceded(multispace0, "["),
        separated(0.., value, preceded(multispace0, ",")),
        preceded(multispace0, "]"),
    )
    .parse_next(input)?;

    Ok(Value::List(values))
}

#[cfg(test)]
mod tests {
    use move_core_types::language_storage::StructTag;

    use super::*;

    fn expect<T: Serialize>(value: T) -> (Vec<u8>, Vec<u8>) {
        let bcs = bcs::to_bytes(&value).unwrap();
        let bin = bincode::encode_to_vec(BorrowCompat(&value), bincode_config()).unwrap();
        (bcs, bin)
    }

    #[test]
    fn test_object_id() {
        let value = ObjectID::from_str("0x1234").unwrap();
        let (bcs, bin) = expect(value);

        assert_eq!(parse("bcs(0x1234)").unwrap(), bcs);
        assert_eq!(parse("bin(0x1234)").unwrap(), bin);
    }

    #[test]
    fn test_number() {
        let value = 42u64;
        let (bcs, bin) = expect(value);

        assert_eq!(parse("bcs(42)").unwrap(), bcs);
        assert_eq!(parse("bin(42)").unwrap(), bin);
    }
    #[test]
    fn test_number_with_suffix() {
        let u8_value = 255u8;
        let (bcs_u8, bin_u8) = expect(u8_value);
        assert_eq!(parse("bcs(255u8)").unwrap(), bcs_u8);
        assert_eq!(parse("bin(255u8)").unwrap(), bin_u8);

        let u16_value = 1000u16;
        let (bcs_u16, bin_u16) = expect(u16_value);
        assert_eq!(parse("bcs(1000u16)").unwrap(), bcs_u16);
        assert_eq!(parse("bin(1000u16)").unwrap(), bin_u16);

        let u128_value = 1_000_000u128;
        let (bcs_u128, bin_u128) = expect(u128_value);
        assert_eq!(parse("bcs(1000000u128)").unwrap(), bcs_u128);
        assert_eq!(parse("bin(1000000u128)").unwrap(), bin_u128);
    }
    #[test]
    fn test_string() {
        let value = "hello world";
        let (bcs, bin) = expect(value);

        assert_eq!(parse("bcs('hello world')").unwrap(), bcs);
        assert_eq!(parse("bin('hello world')").unwrap(), bin);
    }
    #[test]
    fn test_string_with_escaped_quote() {
        let value = "it's working";
        let (bcs, bin) = expect(value);

        assert_eq!(parse("bcs('it\\'s working')").unwrap(), bcs);
        assert_eq!(parse("bin('it\\'s working')").unwrap(), bin);
    }
    #[test]
    fn test_tuple() {
        let value = (42u64, "hello");
        let (bcs, bin) = expect(value);

        assert_eq!(parse("bcs(42, 'hello')").unwrap(), bcs);
        assert_eq!(parse("bin(42, 'hello')").unwrap(), bin);
    }
    #[test]
    fn test_list() {
        let value = vec![1u64, 2u64, 3u64];
        let (bcs, bin) = expect(value);

        assert_eq!(parse("bcs[1, 2, 3]").unwrap(), bcs);
        assert_eq!(parse("bin[1, 2, 3]").unwrap(), bin);
    }
    #[test]
    fn test_empty_list() {
        let value: Vec<u64> = vec![];
        let (bcs, bin) = expect(value);

        assert_eq!(parse("bcs[]").unwrap(), bcs);
        assert_eq!(parse("bin[]").unwrap(), bin);
    }
    #[test]
    fn test_struct_tag() {
        let tag = StructTag::from_str("0x2::table::Table<address, 0x2::coin::Coin<0x2::sui::SUI>>")
            .unwrap();
        let (bcs, bin) = expect(tag);

        assert_eq!(
            parse("bcs(0x2::table::Table<address, 0x2::coin::Coin<0x2::sui::SUI>>)").unwrap(),
            bcs
        );
        assert_eq!(
            parse("bin(0x2::table::Table<address, 0x2::coin::Coin<0x2::sui::SUI>>)").unwrap(),
            bin
        );
    }
}
