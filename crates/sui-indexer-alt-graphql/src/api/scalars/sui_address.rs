// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, str::FromStr};

use async_graphql::{InputValueError, InputValueResult, Scalar, ScalarType, Value};
use move_core_types::account_address::AccountAddress;
use sui_types::base_types::{ObjectID, SuiAddress as NativeSuiAddress};

const SUI_ADDRESS_LENGTH: usize = 32;

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct SuiAddress([u8; SUI_ADDRESS_LENGTH]);

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Invalid hexadecimal character at offset {0}")]
    BadHex(usize),

    #[error("Missing '0x' prefix while parsing SuiAddress: {0:?}")]
    NoPrefix(String),

    #[error(
        "Expected between 1 and {} hexadecimal digits ({} bytes), received {0}",
        SUI_ADDRESS_LENGTH * 2,
        SUI_ADDRESS_LENGTH,
    )]
    WrongLength(usize),
}

/// String containing 32 byte hex-encoded address, with a leading '0x'. Leading zeroes can be omitted on input but will always appear in outputs (SuiAddress in output is guaranteed to be 66 characters long).
#[Scalar]
impl ScalarType for SuiAddress {
    fn parse(value: Value) -> InputValueResult<Self> {
        let Value::String(s) = value else {
            return Err(InputValueError::expected_type(value));
        };

        Ok(SuiAddress::from_str(&s)?)
    }

    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromStr for SuiAddress {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let Some(s) = s.strip_prefix("0x") else {
            return Err(Error::NoPrefix(s.to_owned()));
        };

        if s.is_empty() || s.len() > SUI_ADDRESS_LENGTH * 2 {
            return Err(Error::WrongLength(s.len()));
        }

        // Parse a single hexadecimal character from the string, or return an error pointing to the
        // bad character in the source string.
        let hex = |i: usize| -> Result<u8, Error> {
            u8::from_str_radix(&s[i..=i], 16).map_err(|_| Error::BadHex(i + 2))
        };

        let mut arr = [0u8; SUI_ADDRESS_LENGTH];

        let mut i = arr.len() - 1;
        let mut j = s.len();

        // Keep filling the array from the back until we have one hex digit left to process.
        while j > 2 {
            arr[i] = (hex(j - 2)? << 4) | hex(j - 1)?;
            i -= 1;
            j -= 2;
        }

        // Process the last hex digit, which will be compromised of either two or one character.
        if j == 2 {
            arr[i] = (hex(0)? << 4) | hex(1)?;
        } else {
            arr[i] = hex(0)?;
        }

        Ok(SuiAddress(arr))
    }
}

impl fmt::Display for SuiAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x")?;
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl From<SuiAddress> for AccountAddress {
    fn from(value: SuiAddress) -> Self {
        AccountAddress::new(value.0)
    }
}

impl From<SuiAddress> for NativeSuiAddress {
    fn from(value: SuiAddress) -> Self {
        AccountAddress::from(value).into()
    }
}

impl From<SuiAddress> for ObjectID {
    fn from(value: SuiAddress) -> Self {
        ObjectID::new(value.0)
    }
}

impl From<NativeSuiAddress> for SuiAddress {
    fn from(value: NativeSuiAddress) -> Self {
        SuiAddress(value.to_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::Value;

    const FULL_ADDRESS_STR: &str =
        "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const FULL_ADDRESS: SuiAddress = SuiAddress([
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd,
        0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
        0xcd, 0xef,
    ]);

    const ODD_SHORT_ADDRESS_STR: &str = "0x123456789";
    const EVEN_SHORT_ADDRESS_STR: &str = "0x0123456789";
    const FULL_SHORT_ADDRESS_STR: &str =
        "0x0000000000000000000000000000000000000000000000000000000123456789";
    const SHORT_ADDRESS: SuiAddress = SuiAddress([
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x23, 0x45,
        0x67, 0x89,
    ]);

    #[test]
    fn test_parse_full() {
        let parsed = SuiAddress::from_str(FULL_ADDRESS_STR).unwrap();
        assert_eq!(parsed, FULL_ADDRESS);
    }

    #[test]
    fn test_parse_short_odd() {
        let parsed = SuiAddress::from_str(ODD_SHORT_ADDRESS_STR).unwrap();
        assert_eq!(parsed, SHORT_ADDRESS);
    }

    #[test]
    fn test_parse_short_even() {
        let parsed = SuiAddress::from_str(EVEN_SHORT_ADDRESS_STR).unwrap();
        assert_eq!(parsed, SHORT_ADDRESS);
    }

    #[test]
    fn test_full_to_value() {
        let value = ScalarType::to_value(&FULL_ADDRESS);
        assert_eq!(value, Value::String(FULL_ADDRESS_STR.to_string()));
    }

    #[test]
    fn test_short_to_value() {
        let value = ScalarType::to_value(&SHORT_ADDRESS);
        assert_eq!(value, Value::String(FULL_SHORT_ADDRESS_STR.to_string()));
    }

    #[test]
    fn test_round_trip_full() {
        let value = ScalarType::to_value(&FULL_ADDRESS);
        let parsed_back = ScalarType::parse(value).unwrap();
        assert_eq!(FULL_ADDRESS, parsed_back);
    }

    #[test]
    fn test_round_trip_short() {
        let value = ScalarType::to_value(&SHORT_ADDRESS);
        let parsed_back = ScalarType::parse(value).unwrap();
        assert_eq!(SHORT_ADDRESS, parsed_back);
    }

    #[test]
    fn test_parse_no_prefix() {
        let err = SuiAddress::from_str("123456789").unwrap_err();
        assert!(matches!(err, Error::NoPrefix(_)), "{err:?}");
    }

    #[test]
    fn test_parse_invalid_prefix() {
        let err = SuiAddress::from_str("1x123456789").unwrap_err();
        assert!(matches!(err, Error::NoPrefix(_)), "{err:?}");
    }

    #[test]
    fn test_parse_too_short() {
        let err = SuiAddress::from_str("0x").unwrap_err();
        assert!(matches!(err, Error::WrongLength(0)), "{err:?}");
    }

    #[test]
    fn test_parse_invalid_characters() {
        let mut input = FULL_ADDRESS_STR.to_owned();
        input.replace_range(20..=20, "g");

        let err = SuiAddress::from_str(&input).unwrap_err();
        assert!(matches!(err, Error::BadHex(20)), "{err:?}");
    }

    #[test]
    fn test_unicode_gibberish() {
        let parsed = SuiAddress::from_str("aAௗ0㌀0");
        assert!(parsed.is_err());
    }

    #[test]
    fn test_bad_scalar_type() {
        let input = Value::Number(0x42.into());
        let parsed = <SuiAddress as ScalarType>::parse(input);
        assert!(parsed.is_err());
    }
}
