// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub const ADDRESS: &str = "address";
pub const SIGNER: &str = "signer";
pub const U_8: &str = "u8";
pub const U_16: &str = "u16";
pub const U_32: &str = "u32";
pub const U_64: &str = "u64";
pub const U_128: &str = "u128";
pub const U_256: &str = "u256";
pub const I_8: &str = "i8";
pub const I_16: &str = "i16";
pub const I_32: &str = "i32";
pub const I_64: &str = "i64";
pub const I_128: &str = "i128";
pub const BOOL: &str = "bool";
pub const VECTOR: &str = "vector";

pub const UNSIGNED_INT_SUFFIXES: &[&str] = &[U_8, U_16, U_32, U_64, U_128, U_256];
pub const SIGNED_INT_SUFFIXES: &[&str] = &[I_8, I_16, I_32, I_64, I_128];

pub const PRIMITIVE_TYPES: &[&str] = &[U_8, U_16, U_32, U_64, U_128, U_256, BOOL, VECTOR];

//**************************************************************************************************
// Suffix helpers
//**************************************************************************************************

pub fn has_signed_suffix(s: &str) -> bool {
    SIGNED_INT_SUFFIXES.iter().any(|sfx| s.ends_with(sfx))
}

pub fn has_unsigned_suffix(s: &str) -> bool {
    UNSIGNED_INT_SUFFIXES.iter().any(|sfx| s.ends_with(sfx))
}

//**************************************************************************************************
// Numbers
//**************************************************************************************************

pub use move_core_types::parsing::parser::{
    NumberFormat, parse_address_number as parse_address, parse_u8, parse_u16, parse_u32, parse_u64,
    parse_u128, parse_u256,
};

use std::num::ParseIntError;

// Signed integer parsing. When `negated` is false, the valid range is 0..=MAX; when true, it is
// 0..=abs(MIN). Returns the final signed value directly (negated when requested).
// `ParseIntError` has no public constructor, so we produce one via a known-failing parse.
fn signed_overflow_parse_error() -> ParseIntError {
    "256".parse::<u8>().unwrap_err()
}

macro_rules! define_parse_signed_int {
    ($fname:ident, $parse_unsigned:ident, $unsigned:ty, $signed:ty) => {
        pub fn $fname(s: &str, negated: bool) -> Result<($signed, NumberFormat), ParseIntError> {
            let (magnitude, fmt) = $parse_unsigned(s)?;
            let value = if negated {
                if magnitude > <$signed>::MIN.unsigned_abs() {
                    return Err(signed_overflow_parse_error());
                }
                // We can wrap here safely because the only way to get an overflow is if the
                // magnitude is exactly abs(MIN), in which case the correct negated value is MIN.
                // In that case, the wrapping negation will also produce the correct value.
                (magnitude as $signed).wrapping_neg()
            } else {
                if magnitude > <$signed>::MAX as $unsigned {
                    return Err(signed_overflow_parse_error());
                }
                magnitude as $signed
            };
            Ok((value, fmt))
        }
    };
}

define_parse_signed_int!(parse_i8, parse_u8, u8, i8);
define_parse_signed_int!(parse_i16, parse_u16, u16, i16);
define_parse_signed_int!(parse_i32, parse_u32, u32, i32);
define_parse_signed_int!(parse_i64, parse_u64, u64, i64);
define_parse_signed_int!(parse_i128, parse_u128, u128, i128);
