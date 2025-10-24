// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::NormalizedPackages;
use crate::Result;
use crate::RpcError;
use move_binary_format::normalized;
use prost_types::Value;
use prost_types::value::Kind;
use sui_sdk_types::Command;
use sui_types::MOVE_STDLIB_ADDRESS;
use sui_types::base_types::ObjectID;
use sui_types::base_types::STD_ASCII_MODULE_NAME;
use sui_types::base_types::STD_ASCII_STRUCT_NAME;
use sui_types::base_types::STD_OPTION_MODULE_NAME;
use sui_types::base_types::STD_OPTION_STRUCT_NAME;
use sui_types::base_types::STD_UTF8_MODULE_NAME;
use sui_types::base_types::STD_UTF8_STRUCT_NAME;

type Type = normalized::Type<normalized::RcIdentifier>;

pub(super) fn resolve_literal(
    called_packages: &mut NormalizedPackages,
    commands: &[Command],
    arg_idx: usize,
    value: &Value,
) -> Result<Vec<u8>> {
    let literal_type = determine_literal_type(called_packages, commands, arg_idx)?;

    let mut buf = Vec::new();

    resolve_literal_to_type(&mut buf, &literal_type, value)?;

    Ok(buf)
}

fn determine_literal_type(
    called_packages: &mut NormalizedPackages,
    commands: &[Command],
    arg_idx: usize,
) -> Result<Type> {
    fn set_type(maybe_type: &mut Option<Type>, ty: Type) -> Result<()> {
        match maybe_type {
            Some(literal_type) if literal_type == &ty => {}
            Some(_) => {
                return Err(RpcError::new(
                    tonic::Code::InvalidArgument,
                    "unable to resolve literal as it is used as multiple different types across commands",
                ));
            }
            None => {
                *maybe_type = Some(ty);
            }
        }

        Ok(())
    }
    let mut literal_type = None;

    for (command, idx) in super::find_arg_uses(arg_idx, commands) {
        match (command, idx) {
            (Command::MoveCall(move_call), Some(idx)) => {
                let arg_type = super::arg_type_of_move_call_input(called_packages, move_call, idx)?;
                set_type(&mut literal_type, (*arg_type).clone())?;
            }
            (Command::TransferObjects(_), None) => {
                set_type(&mut literal_type, Type::Address)?;
            }

            (Command::SplitCoins(_), Some(_)) => {
                set_type(&mut literal_type, Type::U64)?;
            }
            (Command::MakeMoveVector(make_move_vector), Some(_)) => {
                if let Some(ty) = &make_move_vector.type_ {
                    let ty =
                        sui_types::sui_sdk_types_conversions::type_tag_sdk_to_core(ty.clone())?;
                    let ty = normalized::Type::from_type_tag(&mut called_packages.pool, &ty);
                    set_type(&mut literal_type, ty)?;
                } else {
                    return Err(RpcError::new(
                        tonic::Code::InvalidArgument,
                        "unable to resolve literal as an unknown type",
                    ));
                }
            }

            // Invalid uses of Literal Arguments

            // Pure arg can't be used as an object to transfer
            (Command::TransferObjects(_), Some(_))
            | (Command::Upgrade(_), _)
            | (Command::MergeCoins(_), _)
            | (Command::SplitCoins(_), None) => {
                return Err(RpcError::new(
                    tonic::Code::InvalidArgument,
                    "invalid use of literal",
                ));
            }

            // bug in find_arg_uses
            (Command::MakeMoveVector(_), None)
            | (Command::Publish(_), _)
            | (Command::MoveCall(_), None) => {
                return Err(RpcError::new(
                    tonic::Code::Internal,
                    "error determining type of literal",
                ));
            }
            _ => return Err(RpcError::new(tonic::Code::Internal, "unknwon command type")),
        }
    }

    literal_type.ok_or_else(|| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            "unable to determine type of literal",
        )
    })
}

fn resolve_literal_to_type(buf: &mut Vec<u8>, type_: &Type, value: &Value) -> Result<()> {
    match type_ {
        Type::Bool => resolve_as_bool(buf, value),
        Type::U8 => resolve_as_number::<u8>(buf, value),
        Type::U16 => resolve_as_number::<u16>(buf, value),
        Type::U32 => resolve_as_number::<u32>(buf, value),
        Type::U64 => resolve_as_number::<u64>(buf, value),
        Type::U128 => resolve_as_number::<u128>(buf, value),
        Type::U256 => resolve_as_number::<move_core_types::u256::U256>(buf, value),
        Type::Address => resolve_as_address(buf, value),

        // 0x1::ascii::String and 0x1::string::String
        Type::Datatype(dt)
            if dt.module.address == MOVE_STDLIB_ADDRESS
                // 0x1::ascii::String
            && ((dt.module.name.as_ref() == STD_ASCII_MODULE_NAME
                && dt.name.as_ref() == STD_ASCII_STRUCT_NAME)
                // 0x1::string::String
                || (dt.module.name.as_ref() == STD_UTF8_MODULE_NAME
                    && dt.name.as_ref() == STD_UTF8_STRUCT_NAME))
            && dt.type_arguments.is_empty() =>
        {
            resolve_as_string(buf, value)
        }

        // Option<T>
        Type::Datatype(dt)
            if dt.module.address == MOVE_STDLIB_ADDRESS
                && dt.module.name.as_ref() == STD_OPTION_MODULE_NAME
                && dt.name.as_ref() == STD_OPTION_STRUCT_NAME
                && dt.type_arguments.len() == 1 =>
        {
            let ty = dt
                .type_arguments
                .first()
                .expect("length of type_arguments is 1");

            resolve_as_option(buf, ty, value)
        }

        // Vec<T>
        Type::Vector(ty) => resolve_as_vector(buf, ty, value),

        Type::Signer | Type::Datatype(_) | Type::TypeParameter(_) | Type::Reference(_, _) => {
            Err(RpcError::new(
                tonic::Code::InvalidArgument,
                format!("literal cannot be resolved into type {type_}"),
            ))
        }
    }
}

fn resolve_as_bool(buf: &mut Vec<u8>, value: &Value) -> Result<()> {
    let b: bool = match &value.kind {
        Some(Kind::BoolValue(b)) => *b,
        Some(Kind::StringValue(s)) => s.parse().map_err(|e| {
            RpcError::new(
                tonic::Code::InvalidArgument,
                format!("literal cannot be resolved as bool: {e}"),
            )
        })?,
        _ => {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "literal cannot be resolved into type bool",
            ));
        }
    };

    bcs::serialize_into(buf, &b)?;

    Ok(())
}

fn resolve_as_number<T>(buf: &mut Vec<u8>, value: &Value) -> Result<()>
where
    T: std::str::FromStr + TryFrom<u64> + serde::Serialize,
    <T as std::str::FromStr>::Err: std::fmt::Display,
    <T as TryFrom<u64>>::Error: std::fmt::Display,
{
    let n: T = match &value.kind {
        Some(Kind::NumberValue(n)) => T::try_from((*n) as u64).map_err(|e| {
            RpcError::new(
                tonic::Code::InvalidArgument,
                format!(
                    "literal cannot be resolved as {}: {e}",
                    std::any::type_name::<T>()
                ),
            )
        })?,

        Some(Kind::StringValue(s)) => s.parse().map_err(|e| {
            RpcError::new(
                tonic::Code::InvalidArgument,
                format!(
                    "literal cannot be resolved as {}: {e}",
                    std::any::type_name::<T>()
                ),
            )
        })?,

        _ => {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                format!(
                    "literal cannot be resolved into type {}",
                    std::any::type_name::<T>()
                ),
            ));
        }
    };

    bcs::serialize_into(buf, &n)?;

    Ok(())
}

fn resolve_as_address(buf: &mut Vec<u8>, value: &Value) -> Result<()> {
    let address = match &value.kind {
        // parse as ObjectID to handle the case where 0x is present or missing
        Some(Kind::StringValue(s)) => s.parse::<ObjectID>().map_err(|e| {
            RpcError::new(
                tonic::Code::InvalidArgument,
                format!("literal cannot be resolved as bool: {e}"),
            )
        })?,
        _ => {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "literal cannot be resolved into type address",
            ));
        }
    };

    bcs::serialize_into(buf, &address)?;

    Ok(())
}

fn resolve_as_string(buf: &mut Vec<u8>, value: &Value) -> Result<()> {
    match &value.kind {
        Some(Kind::StringValue(s)) => {
            bcs::serialize_into(buf, s)?;
        }
        _ => {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "literal cannot be resolved into string",
            ));
        }
    };

    Ok(())
}

fn resolve_as_option(buf: &mut Vec<u8>, type_: &Type, value: &Value) -> Result<()> {
    match &value.kind {
        Some(Kind::NullValue(_)) => {
            buf.push(0);
        }
        Some(Kind::BoolValue(_))
        | Some(Kind::NumberValue(_))
        | Some(Kind::StringValue(_))
        | Some(Kind::ListValue(_)) => {
            buf.push(1);
            resolve_literal_to_type(buf, type_, value)?;
        }
        _ => {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "literal cannot be resolved into Option",
            ));
        }
    }

    Ok(())
}

fn resolve_as_vector(buf: &mut Vec<u8>, type_: &Type, value: &Value) -> Result<()> {
    fn write_u32_as_uleb128(buf: &mut Vec<u8>, mut value: u32) {
        while value >= 0x80 {
            // Write 7 (lowest) bits of data and set the 8th bit to 1.
            let byte = (value & 0x7f) as u8;
            buf.push(byte | 0x80);
            value >>= 7;
        }
        // Write the remaining bits of data and set the highest bit to 0.
        buf.push(value as u8);
    }

    match &value.kind {
        Some(Kind::ListValue(prost_types::ListValue { values })) => {
            write_u32_as_uleb128(buf, values.len() as u32);
            for value in values {
                resolve_literal_to_type(buf, type_, value)?;
            }
        }
        _ => {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                format!("literal cannot be resolved into type Vector<{type_}>"),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use move_binary_format::normalized::{self, Datatype, StringPool};
    use move_core_types::{
        account_address::AccountAddress, language_storage::StructTag, u256::U256,
    };

    type Type = normalized::Type<normalized::RcIdentifier>;

    fn test_resolve_literal<V: Into<Value>>(ty: Type, value: V, expected: Option<Vec<u8>>) {
        let mut buf = Vec::new();
        let value = value.into();
        match (resolve_literal_to_type(&mut buf, &ty, &value), expected) {
            (Ok(_), None) => {
                panic!("resolving literal succeeded but failure was expected: {ty} {value:?}")
            }
            (Ok(()), Some(expected)) => assert_eq!(buf, expected),
            (Err(_), None) => {}
            (Err(_), Some(_)) => {
                panic!("failed to resolve literal {value:?} as {ty}");
            }
        }
    }

    #[test]
    fn resolve_bool() {
        let test_cases = [
            (Type::Bool, Kind::BoolValue(true), Some(vec![1])),
            (Type::Bool, Kind::BoolValue(false), Some(vec![0])),
            (Type::Bool, Kind::StringValue("true".into()), Some(vec![1])),
            (Type::Bool, Kind::StringValue("false".into()), Some(vec![0])),
            (Type::Bool, Kind::NullValue(0), None),
            (Type::Bool, Kind::NumberValue(0.into()), None),
            (
                Type::Bool,
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (Type::Bool, Kind::StringValue("foo".into()), None),
        ];

        for (ty, kind, expected) in test_cases {
            test_resolve_literal(ty, kind, expected);
        }
    }

    #[test]
    fn resolve_number() {
        let test_cases = [
            // U8 Successful cases
            (
                Type::U8,
                Kind::NumberValue(u8::MAX.into()),
                Some(bcs::to_bytes(&u8::MAX).unwrap()),
            ),
            (
                Type::U8,
                Kind::NumberValue(u8::MIN.into()),
                Some(bcs::to_bytes(&u8::MIN).unwrap()),
            ),
            (
                Type::U8,
                Kind::StringValue(u8::MAX.to_string()),
                Some(bcs::to_bytes(&u8::MAX).unwrap()),
            ),
            (
                Type::U8,
                Kind::StringValue(u8::MIN.to_string()),
                Some(bcs::to_bytes(&u8::MIN).unwrap()),
            ),
            // U8 failure cases
            (Type::U8, Kind::BoolValue(true), None),
            (
                Type::U8,
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (Type::U8, Kind::NullValue(0), None),
            (Type::U8, Kind::StringValue("foo".into()), None),
            (Type::U8, Kind::StringValue(u64::MAX.to_string()), None),
            (Type::U8, Kind::NumberValue(u64::MAX as _), None),
            // U16 Successful cases
            (
                Type::U16,
                Kind::NumberValue(u16::MAX.into()),
                Some(bcs::to_bytes(&u16::MAX).unwrap()),
            ),
            (
                Type::U16,
                Kind::NumberValue(u16::MIN.into()),
                Some(bcs::to_bytes(&u16::MIN).unwrap()),
            ),
            (
                Type::U16,
                Kind::StringValue(u16::MAX.to_string()),
                Some(bcs::to_bytes(&u16::MAX).unwrap()),
            ),
            (
                Type::U16,
                Kind::StringValue(u16::MIN.to_string()),
                Some(bcs::to_bytes(&u16::MIN).unwrap()),
            ),
            // U16 failure cases
            (Type::U16, Kind::BoolValue(true), None),
            (
                Type::U16,
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (Type::U16, Kind::NullValue(0), None),
            (Type::U16, Kind::StringValue("foo".into()), None),
            (Type::U16, Kind::StringValue(u64::MAX.to_string()), None),
            (Type::U16, Kind::NumberValue(u64::MAX as _), None),
            // U32 Successful cases
            (
                Type::U32,
                Kind::NumberValue(u32::MAX.into()),
                Some(bcs::to_bytes(&u32::MAX).unwrap()),
            ),
            (
                Type::U32,
                Kind::NumberValue(u32::MIN.into()),
                Some(bcs::to_bytes(&u32::MIN).unwrap()),
            ),
            (
                Type::U32,
                Kind::StringValue(u32::MAX.to_string()),
                Some(bcs::to_bytes(&u32::MAX).unwrap()),
            ),
            (
                Type::U32,
                Kind::StringValue(u32::MIN.to_string()),
                Some(bcs::to_bytes(&u32::MIN).unwrap()),
            ),
            // U32 failure cases
            (Type::U32, Kind::BoolValue(true), None),
            (
                Type::U32,
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (Type::U32, Kind::NullValue(0), None),
            (Type::U32, Kind::StringValue("foo".into()), None),
            (Type::U32, Kind::StringValue(u64::MAX.to_string()), None),
            (Type::U32, Kind::NumberValue(u64::MAX as _), None),
            // U64 Successful cases
            (
                Type::U64,
                Kind::NumberValue(u64::MAX as _),
                Some(bcs::to_bytes(&u64::MAX).unwrap()),
            ),
            (
                Type::U64,
                Kind::NumberValue(u64::MIN as _),
                Some(bcs::to_bytes(&u64::MIN).unwrap()),
            ),
            (
                Type::U64,
                Kind::StringValue(u64::MAX.to_string()),
                Some(bcs::to_bytes(&u64::MAX).unwrap()),
            ),
            (
                Type::U64,
                Kind::StringValue(u64::MIN.to_string()),
                Some(bcs::to_bytes(&u64::MIN).unwrap()),
            ),
            // U64 failure cases
            (Type::U64, Kind::BoolValue(true), None),
            (
                Type::U64,
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (Type::U64, Kind::NullValue(0), None),
            (Type::U64, Kind::StringValue("foo".into()), None),
            (Type::U64, Kind::StringValue(u128::MAX.to_string()), None),
            // U128 Successful cases
            (
                Type::U128,
                Kind::NumberValue(u64::MAX as _),
                Some(bcs::to_bytes(&u128::from(u64::MAX)).unwrap()),
            ),
            (
                Type::U128,
                Kind::NumberValue(u64::MIN as _),
                Some(bcs::to_bytes(&u128::MIN).unwrap()),
            ),
            (
                Type::U128,
                Kind::StringValue(u128::MAX.to_string()),
                Some(bcs::to_bytes(&u128::MAX).unwrap()),
            ),
            (
                Type::U128,
                Kind::StringValue(u128::MIN.to_string()),
                Some(bcs::to_bytes(&u128::MIN).unwrap()),
            ),
            // U128 failure cases
            (Type::U128, Kind::BoolValue(true), None),
            (
                Type::U128,
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (Type::U128, Kind::NullValue(0), None),
            (Type::U128, Kind::StringValue("foo".into()), None),
            (
                Type::U128,
                Kind::StringValue(U256::max_value().to_string()),
                None,
            ),
            // U256 Successful cases
            (
                Type::U256,
                Kind::NumberValue(u64::MAX as _),
                Some(bcs::to_bytes(&U256::from(u64::MAX)).unwrap()),
            ),
            (
                Type::U256,
                Kind::NumberValue(u64::MIN as _),
                Some(bcs::to_bytes(&U256::zero()).unwrap()),
            ),
            (
                Type::U256,
                Kind::StringValue(U256::max_value().to_string()),
                Some(bcs::to_bytes(&U256::max_value()).unwrap()),
            ),
            (
                Type::U256,
                Kind::StringValue(U256::zero().to_string()),
                Some(bcs::to_bytes(&U256::zero()).unwrap()),
            ),
            // U256 failure cases
            (Type::U256, Kind::BoolValue(true), None),
            (
                Type::U256,
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (Type::U256, Kind::NullValue(0), None),
            (Type::U256, Kind::StringValue("foo".into()), None),
        ];

        for (ty, kind, expected) in test_cases {
            test_resolve_literal(ty, kind, expected);
        }
    }

    #[test]
    fn resolve_address() {
        let test_cases = [
            // Address Successful cases
            (
                Type::Address,
                // with 0x prefix
                Kind::StringValue(AccountAddress::TWO.to_canonical_string(true)),
                Some(bcs::to_bytes(&AccountAddress::TWO).unwrap()),
            ),
            (
                Type::Address,
                // without 0x prefix
                Kind::StringValue(AccountAddress::TWO.to_canonical_string(false)),
                Some(bcs::to_bytes(&AccountAddress::TWO).unwrap()),
            ),
            (
                Type::Address,
                // with 0x prefix and trimmed 0s
                Kind::StringValue(AccountAddress::TWO.to_hex_literal()),
                Some(bcs::to_bytes(&AccountAddress::TWO).unwrap()),
            ),
            // Address failure cases
            (Type::Address, Kind::BoolValue(true), None),
            (
                Type::Address,
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (Type::Address, Kind::NullValue(0), None),
            (Type::Address, Kind::StringValue("foo".into()), None),
            (Type::Address, Kind::NumberValue(0 as _), None),
            (
                Type::Address,
                // without 0x prefix and with trimmed 0s
                Kind::StringValue(AccountAddress::TWO.short_str_lossless()),
                None,
            ),
        ];

        for (ty, kind, expected) in test_cases {
            test_resolve_literal(ty, kind, expected);
        }
    }

    #[test]
    fn resolve_string() {
        fn utf8() -> Type {
            Type::from_struct_tag(
                &mut normalized::RcPool::new(),
                &StructTag {
                    address: MOVE_STDLIB_ADDRESS,
                    module: STD_UTF8_MODULE_NAME.to_owned(),
                    name: STD_UTF8_STRUCT_NAME.to_owned(),
                    type_params: vec![],
                },
            )
        }
        fn ascii() -> Type {
            Type::from_struct_tag(
                &mut normalized::RcPool::new(),
                &StructTag {
                    address: MOVE_STDLIB_ADDRESS,
                    module: STD_ASCII_MODULE_NAME.to_owned(),
                    name: STD_ASCII_STRUCT_NAME.to_owned(),
                    type_params: vec![],
                },
            )
        }

        let test_cases = [
            // string Successful cases
            (
                utf8(),
                Kind::StringValue("foo".into()),
                Some(bcs::to_bytes(&"foo").unwrap()),
            ),
            (
                ascii(),
                Kind::StringValue("foo".into()),
                Some(bcs::to_bytes(&"foo").unwrap()),
            ),
            (
                utf8(),
                Kind::StringValue("".into()),
                Some(bcs::to_bytes(&"").unwrap()),
            ),
            (
                ascii(),
                Kind::StringValue("".into()),
                Some(bcs::to_bytes(&"").unwrap()),
            ),
            // String failure cases
            (utf8(), Kind::BoolValue(true), None),
            (
                utf8(),
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (utf8(), Kind::NullValue(0), None),
            (utf8(), Kind::NumberValue(0 as _), None),
            (ascii(), Kind::BoolValue(true), None),
            (
                ascii(),
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                None,
            ),
            (ascii(), Kind::NullValue(0), None),
            (ascii(), Kind::NumberValue(0 as _), None),
        ];

        for (ty, kind, expected) in test_cases {
            test_resolve_literal(ty, kind, expected);
        }
    }

    #[test]
    fn resolve_option() {
        fn option_type(t: Type) -> Type {
            let pool = &mut normalized::RcPool::new();
            Type::Datatype(Box::new(Datatype {
                module: normalized::ModuleId {
                    address: MOVE_STDLIB_ADDRESS,
                    name: pool.intern(STD_OPTION_MODULE_NAME),
                },
                name: pool.intern(STD_OPTION_STRUCT_NAME),
                type_arguments: vec![t],
            }))
        }

        let test_cases = [
            // Option Successful cases
            (
                option_type(Type::Address),
                Kind::StringValue(AccountAddress::TWO.to_canonical_string(true)),
                Some(bcs::to_bytes(&Some(AccountAddress::TWO)).unwrap()),
            ),
            (
                option_type(Type::Address),
                Kind::NullValue(0),
                Some(vec![0]),
            ),
            (
                option_type(Type::U64),
                Kind::NumberValue(u64::MIN as _),
                Some(bcs::to_bytes(&Some(u64::MIN)).unwrap()),
            ),
            (
                option_type(Type::U64),
                Kind::StringValue(u64::MAX.to_string()),
                Some(bcs::to_bytes(&Some(u64::MAX)).unwrap()),
            ),
            (
                option_type(Type::Bool),
                Kind::BoolValue(true),
                Some(bcs::to_bytes(&Some(true)).unwrap()),
            ),
            (option_type(Type::Bool), Kind::NullValue(0), Some(vec![0])),
            // Option failure cases
            (option_type(Type::Bool), Kind::NumberValue(0 as _), None),
        ];

        for (ty, kind, expected) in test_cases {
            test_resolve_literal(ty, kind, expected);
        }
    }

    #[test]
    fn resolve_vector() {
        fn vector_type(t: Type) -> Type {
            Type::Vector(Box::new(t))
        }

        let test_cases = [
            // Vector Successful cases
            (
                vector_type(Type::Address),
                Kind::ListValue(prost_types::ListValue {
                    values: vec![
                        Kind::StringValue(AccountAddress::TWO.to_canonical_string(true)).into(),
                        Kind::StringValue(AccountAddress::ONE.to_canonical_string(true)).into(),
                    ],
                }),
                Some(bcs::to_bytes(&vec![AccountAddress::TWO, AccountAddress::ONE]).unwrap()),
            ),
            (
                vector_type(Type::U8),
                Kind::ListValue(prost_types::ListValue {
                    values: vec![Kind::NumberValue(9 as _).into()],
                }),
                Some(vec![1, 9]),
            ),
            (
                vector_type(Type::U8),
                Kind::ListValue(prost_types::ListValue { values: vec![] }),
                Some(vec![0]),
            ),
            (
                vector_type(vector_type(Type::U8)),
                Kind::ListValue(prost_types::ListValue {
                    values: vec![
                        Kind::ListValue(prost_types::ListValue {
                            values: vec![Kind::NumberValue(9 as _).into()],
                        })
                        .into(),
                    ],
                }),
                Some(bcs::to_bytes(&vec![vec![9u8]]).unwrap()),
            ),
            (
                vector_type(Type::Bool),
                // verify we handle uleb128 encoding of length properly
                Kind::ListValue(prost_types::ListValue {
                    values: vec![Kind::BoolValue(true).into(); 256],
                }),
                Some(bcs::to_bytes(&vec![true; 256]).unwrap()),
            ),
            // Vector failure cases
            (vector_type(Type::U64), Kind::BoolValue(true), None),
            (vector_type(Type::U64), Kind::NumberValue(0 as _), None),
            (vector_type(Type::U64), Kind::NullValue(0), None),
            (vector_type(Type::U64), Kind::NumberValue(0 as _), None),
            (
                vector_type(Type::Address),
                Kind::ListValue(prost_types::ListValue {
                    values: vec![
                        Kind::StringValue(AccountAddress::TWO.to_canonical_string(true)).into(),
                        Kind::NumberValue(5 as _).into(),
                    ],
                }),
                None,
            ),
        ];

        for (ty, kind, expected) in test_cases {
            test_resolve_literal(ty, kind, expected);
        }
    }
}
