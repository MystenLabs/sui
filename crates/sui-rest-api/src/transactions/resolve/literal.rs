// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use super::NormalizedPackage;
use crate::RestError;
use crate::Result;
use move_binary_format::normalized::Type;
use sui_sdk_types::types::Command;
use sui_sdk_types::types::ObjectId;
use sui_sdk_types::types::UnresolvedValue;
use sui_types::base_types::ObjectID;
use sui_types::base_types::STD_ASCII_MODULE_NAME;
use sui_types::base_types::STD_ASCII_STRUCT_NAME;
use sui_types::base_types::STD_OPTION_MODULE_NAME;
use sui_types::base_types::STD_OPTION_STRUCT_NAME;
use sui_types::base_types::STD_UTF8_MODULE_NAME;
use sui_types::base_types::STD_UTF8_STRUCT_NAME;
use sui_types::MOVE_STDLIB_ADDRESS;

pub(super) fn resolve_literal(
    called_packages: &HashMap<ObjectId, NormalizedPackage>,
    commands: &[Command],
    arg_idx: usize,
    value: UnresolvedValue,
) -> Result<Vec<u8>> {
    let literal_type = determine_literal_type(called_packages, commands, arg_idx)?;

    let mut buf = Vec::new();

    resolve_literal_to_type(&mut buf, &literal_type, &value)?;

    Ok(buf)
}

fn determine_literal_type(
    called_packages: &HashMap<ObjectId, NormalizedPackage>,
    commands: &[Command],
    arg_idx: usize,
) -> Result<Type> {
    fn set_type(maybe_type: &mut Option<Type>, ty: Type) -> Result<()> {
        match maybe_type {
            Some(literal_type) if literal_type == &ty => {}
            Some(_) => {
                return Err(RestError::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "unable to resolve literal as it is used as multiple different types across commands",
                ))
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
                set_type(&mut literal_type, arg_type.to_owned())?;
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
                    set_type(&mut literal_type, ty.into())?;
                } else {
                    return Err(RestError::new(
                        axum::http::StatusCode::BAD_REQUEST,
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
                return Err(RestError::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "invalid use of literal",
                ));
            }

            // bug in find_arg_uses
            (Command::MakeMoveVector(_), None)
            | (Command::Publish(_), _)
            | (Command::MoveCall(_), None) => {
                return Err(RestError::new(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "error determining type of literal",
                ));
            }
        }
    }

    literal_type.ok_or_else(|| {
        RestError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "unable to determine type of literal",
        )
    })
}

fn resolve_literal_to_type(buf: &mut Vec<u8>, type_: &Type, value: &UnresolvedValue) -> Result<()> {
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
        Type::Struct {
            address,
            module,
            name,
            type_arguments,
        } if address == &MOVE_STDLIB_ADDRESS
                // 0x1::ascii::String
            && ((module.as_ref() == STD_ASCII_MODULE_NAME
                && name.as_ref() == STD_ASCII_STRUCT_NAME)
                // 0x1::string::String
                || (module.as_ref() == STD_UTF8_MODULE_NAME
                    && name.as_ref() == STD_UTF8_STRUCT_NAME))
            && type_arguments.is_empty() =>
        {
            resolve_as_string(buf, value)
        }

        // Option<T>
        Type::Struct {
            address,
            module,
            name,
            type_arguments,
        } if address == &MOVE_STDLIB_ADDRESS
            && module.as_ref() == STD_OPTION_MODULE_NAME
            && name.as_ref() == STD_OPTION_STRUCT_NAME
            && type_arguments.len() == 1 =>
        {
            let ty = type_arguments
                .first()
                .expect("length of type_arguments is 1");

            resolve_as_option(buf, ty, value)
        }

        // Vec<T>
        Type::Vector(ty) => resolve_as_vector(buf, ty, value),

        Type::Signer
        | Type::Struct { .. }
        | Type::TypeParameter(_)
        | Type::Reference(_)
        | Type::MutableReference(_) => Err(RestError::new(
            axum::http::StatusCode::BAD_REQUEST,
            format!("literal cannot be resolved into type {type_}"),
        )),
    }
}

fn resolve_as_bool(buf: &mut Vec<u8>, value: &UnresolvedValue) -> Result<()> {
    let b: bool = match value {
        UnresolvedValue::Bool(b) => *b,
        UnresolvedValue::String(s) => s.parse().map_err(|e| {
            RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                format!("literal cannot be resolved as bool: {e}"),
            )
        })?,
        UnresolvedValue::Null | UnresolvedValue::Number(_) | UnresolvedValue::Array(_) => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "literal cannot be resolved into type bool",
            ))
        }
    };

    bcs::serialize_into(buf, &b)?;

    Ok(())
}

fn resolve_as_number<T>(buf: &mut Vec<u8>, value: &UnresolvedValue) -> Result<()>
where
    T: std::str::FromStr + TryFrom<u64> + serde::Serialize,
    <T as std::str::FromStr>::Err: std::fmt::Display,
    <T as TryFrom<u64>>::Error: std::fmt::Display,
{
    let n: T = match value {
        UnresolvedValue::Number(n) => T::try_from(*n).map_err(|e| {
            RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                format!(
                    "literal cannot be resolved as {}: {e}",
                    std::any::type_name::<T>()
                ),
            )
        })?,

        UnresolvedValue::String(s) => s.parse().map_err(|e| {
            RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                format!(
                    "literal cannot be resolved as {}: {e}",
                    std::any::type_name::<T>()
                ),
            )
        })?,

        UnresolvedValue::Null | UnresolvedValue::Bool(_) | UnresolvedValue::Array(_) => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                format!(
                    "literal cannot be resolved into type {}",
                    std::any::type_name::<T>()
                ),
            ))
        }
    };

    bcs::serialize_into(buf, &n)?;

    Ok(())
}

fn resolve_as_address(buf: &mut Vec<u8>, value: &UnresolvedValue) -> Result<()> {
    let address = match value {
        // parse as ObjectID to handle the case where 0x is present or missing
        UnresolvedValue::String(s) => s.parse::<ObjectID>().map_err(|e| {
            RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                format!("literal cannot be resolved as bool: {e}"),
            )
        })?,
        UnresolvedValue::Null
        | UnresolvedValue::Bool(_)
        | UnresolvedValue::Number(_)
        | UnresolvedValue::Array(_) => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "literal cannot be resolved into type address",
            ))
        }
    };

    bcs::serialize_into(buf, &address)?;

    Ok(())
}

fn resolve_as_string(buf: &mut Vec<u8>, value: &UnresolvedValue) -> Result<()> {
    match value {
        UnresolvedValue::String(s) => {
            bcs::serialize_into(buf, s)?;
        }
        UnresolvedValue::Bool(_)
        | UnresolvedValue::Null
        | UnresolvedValue::Number(_)
        | UnresolvedValue::Array(_) => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "literal cannot be resolved into string",
            ))
        }
    };

    Ok(())
}

fn resolve_as_option(buf: &mut Vec<u8>, type_: &Type, value: &UnresolvedValue) -> Result<()> {
    match value {
        UnresolvedValue::Null => {
            buf.push(0);
        }
        UnresolvedValue::Bool(_)
        | UnresolvedValue::Number(_)
        | UnresolvedValue::String(_)
        | UnresolvedValue::Array(_) => {
            buf.push(1);
            resolve_literal_to_type(buf, type_, value)?;
        }
    }

    Ok(())
}

fn resolve_as_vector(buf: &mut Vec<u8>, type_: &Type, value: &UnresolvedValue) -> Result<()> {
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

    match value {
        UnresolvedValue::Array(array) => {
            write_u32_as_uleb128(buf, array.len() as u32);
            for value in array {
                resolve_literal_to_type(buf, type_, value)?;
            }
        }
        UnresolvedValue::Bool(_)
        | UnresolvedValue::Number(_)
        | UnresolvedValue::String(_)
        | UnresolvedValue::Null => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                format!("literal cannot be resolved into type Vector<{type_}>"),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use move_binary_format::normalized::Type;
    use move_core_types::{account_address::AccountAddress, u256::U256};

    fn test_resolve_literal(ty: Type, value: UnresolvedValue, expected: Option<Vec<u8>>) {
        let mut buf = Vec::new();
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
            (Type::Bool, UnresolvedValue::Bool(true), Some(vec![1])),
            (Type::Bool, UnresolvedValue::Bool(false), Some(vec![0])),
            (
                Type::Bool,
                UnresolvedValue::String("true".into()),
                Some(vec![1]),
            ),
            (
                Type::Bool,
                UnresolvedValue::String("false".into()),
                Some(vec![0]),
            ),
            (Type::Bool, UnresolvedValue::Null, None),
            (Type::Bool, UnresolvedValue::Number(0), None),
            (Type::Bool, UnresolvedValue::Array(vec![]), None),
            (Type::Bool, UnresolvedValue::String("foo".into()), None),
        ];

        for (ty, value, expected) in test_cases {
            test_resolve_literal(ty, value, expected);
        }
    }

    #[test]
    fn resolve_number() {
        let test_cases = [
            // U8 Successful cases
            (
                Type::U8,
                UnresolvedValue::Number(u8::MAX.into()),
                Some(bcs::to_bytes(&u8::MAX).unwrap()),
            ),
            (
                Type::U8,
                UnresolvedValue::Number(u8::MIN.into()),
                Some(bcs::to_bytes(&u8::MIN).unwrap()),
            ),
            (
                Type::U8,
                UnresolvedValue::String(u8::MAX.to_string()),
                Some(bcs::to_bytes(&u8::MAX).unwrap()),
            ),
            (
                Type::U8,
                UnresolvedValue::String(u8::MIN.to_string()),
                Some(bcs::to_bytes(&u8::MIN).unwrap()),
            ),
            // U8 failure cases
            (Type::U8, UnresolvedValue::Bool(true), None),
            (Type::U8, UnresolvedValue::Array(vec![]), None),
            (Type::U8, UnresolvedValue::Null, None),
            (Type::U8, UnresolvedValue::String("foo".into()), None),
            (
                Type::U8,
                UnresolvedValue::String(u64::MAX.to_string()),
                None,
            ),
            (Type::U8, UnresolvedValue::Number(u64::MAX), None),
            // U16 Successful cases
            (
                Type::U16,
                UnresolvedValue::Number(u16::MAX.into()),
                Some(bcs::to_bytes(&u16::MAX).unwrap()),
            ),
            (
                Type::U16,
                UnresolvedValue::Number(u16::MIN.into()),
                Some(bcs::to_bytes(&u16::MIN).unwrap()),
            ),
            (
                Type::U16,
                UnresolvedValue::String(u16::MAX.to_string()),
                Some(bcs::to_bytes(&u16::MAX).unwrap()),
            ),
            (
                Type::U16,
                UnresolvedValue::String(u16::MIN.to_string()),
                Some(bcs::to_bytes(&u16::MIN).unwrap()),
            ),
            // U16 failure cases
            (Type::U16, UnresolvedValue::Bool(true), None),
            (Type::U16, UnresolvedValue::Array(vec![]), None),
            (Type::U16, UnresolvedValue::Null, None),
            (Type::U16, UnresolvedValue::String("foo".into()), None),
            (
                Type::U16,
                UnresolvedValue::String(u64::MAX.to_string()),
                None,
            ),
            (Type::U16, UnresolvedValue::Number(u64::MAX), None),
            // U32 Successful cases
            (
                Type::U32,
                UnresolvedValue::Number(u32::MAX.into()),
                Some(bcs::to_bytes(&u32::MAX).unwrap()),
            ),
            (
                Type::U32,
                UnresolvedValue::Number(u32::MIN.into()),
                Some(bcs::to_bytes(&u32::MIN).unwrap()),
            ),
            (
                Type::U32,
                UnresolvedValue::String(u32::MAX.to_string()),
                Some(bcs::to_bytes(&u32::MAX).unwrap()),
            ),
            (
                Type::U32,
                UnresolvedValue::String(u32::MIN.to_string()),
                Some(bcs::to_bytes(&u32::MIN).unwrap()),
            ),
            // U32 failure cases
            (Type::U32, UnresolvedValue::Bool(true), None),
            (Type::U32, UnresolvedValue::Array(vec![]), None),
            (Type::U32, UnresolvedValue::Null, None),
            (Type::U32, UnresolvedValue::String("foo".into()), None),
            (
                Type::U32,
                UnresolvedValue::String(u64::MAX.to_string()),
                None,
            ),
            (Type::U32, UnresolvedValue::Number(u64::MAX), None),
            // U64 Successful cases
            (
                Type::U64,
                UnresolvedValue::Number(u64::MAX),
                Some(bcs::to_bytes(&u64::MAX).unwrap()),
            ),
            (
                Type::U64,
                UnresolvedValue::Number(u64::MIN),
                Some(bcs::to_bytes(&u64::MIN).unwrap()),
            ),
            (
                Type::U64,
                UnresolvedValue::String(u64::MAX.to_string()),
                Some(bcs::to_bytes(&u64::MAX).unwrap()),
            ),
            (
                Type::U64,
                UnresolvedValue::String(u64::MIN.to_string()),
                Some(bcs::to_bytes(&u64::MIN).unwrap()),
            ),
            // U64 failure cases
            (Type::U64, UnresolvedValue::Bool(true), None),
            (Type::U64, UnresolvedValue::Array(vec![]), None),
            (Type::U64, UnresolvedValue::Null, None),
            (Type::U64, UnresolvedValue::String("foo".into()), None),
            (
                Type::U64,
                UnresolvedValue::String(u128::MAX.to_string()),
                None,
            ),
            // U128 Successful cases
            (
                Type::U128,
                UnresolvedValue::Number(u64::MAX),
                Some(bcs::to_bytes(&u128::from(u64::MAX)).unwrap()),
            ),
            (
                Type::U128,
                UnresolvedValue::Number(u64::MIN),
                Some(bcs::to_bytes(&u128::MIN).unwrap()),
            ),
            (
                Type::U128,
                UnresolvedValue::String(u128::MAX.to_string()),
                Some(bcs::to_bytes(&u128::MAX).unwrap()),
            ),
            (
                Type::U128,
                UnresolvedValue::String(u128::MIN.to_string()),
                Some(bcs::to_bytes(&u128::MIN).unwrap()),
            ),
            // U128 failure cases
            (Type::U128, UnresolvedValue::Bool(true), None),
            (Type::U128, UnresolvedValue::Array(vec![]), None),
            (Type::U128, UnresolvedValue::Null, None),
            (Type::U128, UnresolvedValue::String("foo".into()), None),
            (
                Type::U128,
                UnresolvedValue::String(U256::max_value().to_string()),
                None,
            ),
            // U256 Successful cases
            (
                Type::U256,
                UnresolvedValue::Number(u64::MAX),
                Some(bcs::to_bytes(&U256::from(u64::MAX)).unwrap()),
            ),
            (
                Type::U256,
                UnresolvedValue::Number(u64::MIN),
                Some(bcs::to_bytes(&U256::zero()).unwrap()),
            ),
            (
                Type::U256,
                UnresolvedValue::String(U256::max_value().to_string()),
                Some(bcs::to_bytes(&U256::max_value()).unwrap()),
            ),
            (
                Type::U256,
                UnresolvedValue::String(U256::zero().to_string()),
                Some(bcs::to_bytes(&U256::zero()).unwrap()),
            ),
            // U256 failure cases
            (Type::U256, UnresolvedValue::Bool(true), None),
            (Type::U256, UnresolvedValue::Array(vec![]), None),
            (Type::U256, UnresolvedValue::Null, None),
            (Type::U256, UnresolvedValue::String("foo".into()), None),
        ];

        for (ty, value, expected) in test_cases {
            test_resolve_literal(ty, value, expected);
        }
    }

    #[test]
    fn resolve_address() {
        let test_cases = [
            // Address Successful cases
            (
                Type::Address,
                // with 0x prefix
                UnresolvedValue::String(AccountAddress::TWO.to_canonical_string(true)),
                Some(bcs::to_bytes(&AccountAddress::TWO).unwrap()),
            ),
            (
                Type::Address,
                // without 0x prefix
                UnresolvedValue::String(AccountAddress::TWO.to_canonical_string(false)),
                Some(bcs::to_bytes(&AccountAddress::TWO).unwrap()),
            ),
            (
                Type::Address,
                // with 0x prefix and trimmed 0s
                UnresolvedValue::String(AccountAddress::TWO.to_hex_literal()),
                Some(bcs::to_bytes(&AccountAddress::TWO).unwrap()),
            ),
            // Address failure cases
            (Type::Address, UnresolvedValue::Bool(true), None),
            (Type::Address, UnresolvedValue::Array(vec![]), None),
            (Type::Address, UnresolvedValue::Null, None),
            (Type::Address, UnresolvedValue::String("foo".into()), None),
            (Type::Address, UnresolvedValue::Number(0), None),
            (
                Type::Address,
                // without 0x prefix and with trimmed 0s
                UnresolvedValue::String(AccountAddress::TWO.short_str_lossless()),
                None,
            ),
        ];

        for (ty, value, expected) in test_cases {
            test_resolve_literal(ty, value, expected);
        }
    }

    #[test]
    fn resolve_string() {
        fn utf8() -> Type {
            Type::Struct {
                address: MOVE_STDLIB_ADDRESS,
                module: STD_UTF8_MODULE_NAME.to_owned(),
                name: STD_UTF8_STRUCT_NAME.to_owned(),
                type_arguments: vec![],
            }
        }
        fn ascii() -> Type {
            Type::Struct {
                address: MOVE_STDLIB_ADDRESS,
                module: STD_ASCII_MODULE_NAME.to_owned(),
                name: STD_ASCII_STRUCT_NAME.to_owned(),
                type_arguments: vec![],
            }
        }

        let test_cases = [
            // string Successful cases
            (
                utf8(),
                UnresolvedValue::String("foo".into()),
                Some(bcs::to_bytes(&"foo").unwrap()),
            ),
            (
                ascii(),
                UnresolvedValue::String("foo".into()),
                Some(bcs::to_bytes(&"foo").unwrap()),
            ),
            (
                utf8(),
                UnresolvedValue::String("".into()),
                Some(bcs::to_bytes(&"").unwrap()),
            ),
            (
                ascii(),
                UnresolvedValue::String("".into()),
                Some(bcs::to_bytes(&"").unwrap()),
            ),
            // String failure cases
            (utf8(), UnresolvedValue::Bool(true), None),
            (utf8(), UnresolvedValue::Array(vec![]), None),
            (utf8(), UnresolvedValue::Null, None),
            (utf8(), UnresolvedValue::Number(0), None),
            (ascii(), UnresolvedValue::Bool(true), None),
            (ascii(), UnresolvedValue::Array(vec![]), None),
            (ascii(), UnresolvedValue::Null, None),
            (ascii(), UnresolvedValue::Number(0), None),
        ];

        for (ty, value, expected) in test_cases {
            test_resolve_literal(ty, value, expected);
        }
    }

    #[test]
    fn resolve_option() {
        fn option_type(t: Type) -> Type {
            Type::Struct {
                address: MOVE_STDLIB_ADDRESS,
                module: STD_OPTION_MODULE_NAME.to_owned(),
                name: STD_OPTION_STRUCT_NAME.to_owned(),
                type_arguments: vec![t],
            }
        }

        let test_cases = [
            // Option Successful cases
            (
                option_type(Type::Address),
                UnresolvedValue::String(AccountAddress::TWO.to_canonical_string(true)),
                Some(bcs::to_bytes(&Some(AccountAddress::TWO)).unwrap()),
            ),
            (
                option_type(Type::Address),
                UnresolvedValue::Null,
                Some(vec![0]),
            ),
            (
                option_type(Type::U64),
                UnresolvedValue::Number(u64::MIN),
                Some(bcs::to_bytes(&Some(u64::MIN)).unwrap()),
            ),
            (
                option_type(Type::U64),
                UnresolvedValue::String(u64::MAX.to_string()),
                Some(bcs::to_bytes(&Some(u64::MAX)).unwrap()),
            ),
            (
                option_type(Type::Bool),
                UnresolvedValue::Bool(true),
                Some(bcs::to_bytes(&Some(true)).unwrap()),
            ),
            (
                option_type(Type::Bool),
                UnresolvedValue::Null,
                Some(vec![0]),
            ),
            // Option failure cases
            (option_type(Type::Bool), UnresolvedValue::Number(0), None),
        ];

        for (ty, value, expected) in test_cases {
            test_resolve_literal(ty, value, expected);
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
                UnresolvedValue::Array(vec![
                    UnresolvedValue::String(AccountAddress::TWO.to_canonical_string(true)),
                    UnresolvedValue::String(AccountAddress::ONE.to_canonical_string(true)),
                ]),
                Some(bcs::to_bytes(&vec![AccountAddress::TWO, AccountAddress::ONE]).unwrap()),
            ),
            (
                vector_type(Type::U8),
                UnresolvedValue::Array(vec![UnresolvedValue::Number(9)]),
                Some(vec![1, 9]),
            ),
            (
                vector_type(Type::U8),
                UnresolvedValue::Array(vec![]),
                Some(vec![0]),
            ),
            (
                vector_type(vector_type(Type::U8)),
                UnresolvedValue::Array(vec![UnresolvedValue::Array(vec![
                    UnresolvedValue::Number(9),
                ])]),
                Some(bcs::to_bytes(&vec![vec![9u8]]).unwrap()),
            ),
            (
                vector_type(Type::Bool),
                // verify we handle uleb128 encoding of length properly
                UnresolvedValue::Array(vec![UnresolvedValue::Bool(true); 256]),
                Some(bcs::to_bytes(&vec![true; 256]).unwrap()),
            ),
            // Vector failure cases
            (vector_type(Type::U64), UnresolvedValue::Bool(true), None),
            (vector_type(Type::U64), UnresolvedValue::Number(0), None),
            (vector_type(Type::U64), UnresolvedValue::Null, None),
            (vector_type(Type::U64), UnresolvedValue::Number(0), None),
            (
                vector_type(Type::Address),
                UnresolvedValue::Array(vec![
                    UnresolvedValue::String(AccountAddress::TWO.to_canonical_string(true)),
                    UnresolvedValue::Number(5),
                ]),
                None,
            ),
        ];

        for (ty, value, expected) in test_cases {
            test_resolve_literal(ty, value, expected);
        }
    }
}
