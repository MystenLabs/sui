// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use serde::{Deserialize, Serialize};

const SUI_ADDRESS_LENGTH: usize = 32;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Copy)]
pub(crate) struct SuiAddress([u8; SUI_ADDRESS_LENGTH]);
#[Scalar]
impl ScalarType for SuiAddress {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::String(mut s) => {
                if s.starts_with("0x") {
                    s = s[2..].to_string();
                } else {
                    return Err(InputValueError::custom(
                        "Invalid SuiAddress. Missing 0x prefix",
                    ));
                }
                if s.is_empty() || s.len() > SUI_ADDRESS_LENGTH * 2 {
                    return Err(InputValueError::custom(format!(
                        "Expected SuiAddress string ranging from length 1 up to {} ({} bytes) or less, received {}.",
                        SUI_ADDRESS_LENGTH * 2,
                        SUI_ADDRESS_LENGTH,
                        s.len()
                    )));
                }
                // Pad to SUI_ADDRESS_LENGTH*2 width
                s = format!("{:0>width$}", s, width = SUI_ADDRESS_LENGTH * 2);

                let bytes = hex::decode(s)?;
                let mut arr = [0u8; SUI_ADDRESS_LENGTH];
                arr.copy_from_slice(&bytes);
                Ok(SuiAddress(arr))
            }
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(format!("0x{}", hex::encode(self.0)))
    }
}

impl SuiAddress {
    pub fn into_array(self) -> [u8; SUI_ADDRESS_LENGTH] {
        self.0
    }

    pub fn from_array(arr: [u8; SUI_ADDRESS_LENGTH]) -> Self {
        SuiAddress(arr)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::Value;

    fn assert_input_value_error<T>(result: Result<T, InputValueError<T>>) {
        match result {
            Err(InputValueError { .. }) => {}
            _ => panic!("Expected InputValueError"),
        }
    }

    #[test]
    fn test_parse_valid_suiaddress() {
        let input = Value::String(
            "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
        let parsed = <SuiAddress as ScalarType>::parse(input).unwrap();
        assert_eq!(
            parsed.0,
            [
                1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69,
                103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239
            ]
        );
    }

    #[test]
    fn test_to_value() {
        let addr = SuiAddress([
            1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103,
            137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239,
        ]);
        let value = <SuiAddress as ScalarType>::to_value(&addr);
        assert_eq!(
            value,
            Value::String(
                "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()
            )
        );
    }

    #[test]
    fn test_into_array() {
        let addr = SuiAddress([
            1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103,
            137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239,
        ]);
        let arr = addr.into_array();
        assert_eq!(
            arr,
            [
                1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69,
                103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239
            ]
        );
    }

    #[test]
    fn test_from_array() {
        let arr = [
            1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103,
            137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239,
        ];
        let addr = SuiAddress::from_array(arr);
        assert_eq!(
            addr,
            SuiAddress([
                1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69,
                103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239
            ])
        );
    }

    #[test]
    fn test_as_slice() {
        let addr = SuiAddress([
            1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103,
            137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239,
        ]);
        let slice = addr.as_slice();
        assert_eq!(
            slice,
            &[
                1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69,
                103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239
            ]
        );
    }

    #[test]
    fn test_round_trip() {
        let addr = SuiAddress([
            1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239, 1, 35, 69, 103,
            137, 171, 205, 239, 1, 35, 69, 103, 137, 171, 205, 239,
        ]);
        let value = <SuiAddress as ScalarType>::to_value(&addr);
        let parsed_back = <SuiAddress as ScalarType>::parse(value).unwrap();
        assert_eq!(addr, parsed_back);
    }

    #[test]
    fn test_parse_no_prefix() {
        let input = Value::String(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
        let parsed = <SuiAddress as ScalarType>::parse(input);
        assert_input_value_error(parsed);
    }

    #[test]
    fn test_parse_invalid_prefix() {
        let input = Value::String(
            "1x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
        let parsed = <SuiAddress as ScalarType>::parse(input);
        assert_input_value_error(parsed);
    }

    #[test]
    fn test_parse_invalid_length() {
        let input = Value::String(
            "0x0123456789abcdef0123456789abcdef01000023456789abcdef0123456789abcdef".to_string(),
        );
        let parsed = <SuiAddress as ScalarType>::parse(input);
        assert_input_value_error(parsed);
    }

    #[test]
    fn test_parse_invalid_characters() {
        let input = Value::String(
            "0x0123456789abcdefg0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
        let parsed = <SuiAddress as ScalarType>::parse(input);
        assert_input_value_error(parsed);
    }

    #[test]
    fn test_unicode_gibberish() {
        let input = Value::String("aAௗ0㌀0".to_string());
        let parsed = <SuiAddress as ScalarType>::parse(input);
        assert_input_value_error(parsed);
    }
}
