// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::*;
use fastcrypto::encoding::{Base64 as NativeBase64, Encoding as _};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Base64(pub(crate) Vec<u8>);

/// String containing Base64-encoded binary data.
#[Scalar]
impl ScalarType for Base64 {
    fn parse(value: Value) -> InputValueResult<Self> {
        let Value::String(s) = value else {
            return Err(InputValueError::expected_type(value));
        };

        Base64::from_str(&s)
    }

    fn to_value(&self) -> Value {
        Value::String(NativeBase64::encode(self.0.clone()))
    }
}

impl FromStr for Base64 {
    type Err = InputValueError<Self>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Base64(
            NativeBase64::decode(s).map_err(|_| InputValueError::custom("Invalid Base64"))?,
        ))
    }
}

impl From<&Vec<u8>> for Base64 {
    fn from(bytes: &Vec<u8>) -> Self {
        Base64(bytes.clone())
    }
}

impl From<&[u8]> for Base64 {
    fn from(bytes: &[u8]) -> Self {
        Base64(bytes.to_vec())
    }
}

impl From<Vec<u8>> for Base64 {
    fn from(bytes: Vec<u8>) -> Self {
        Base64(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_base64() {
        let input = Value::String("SGVsbG8gd29ybGQ=".to_string());
        let parsed = <Base64 as ScalarType>::parse(input).unwrap();
        assert_eq!(parsed.0, b"Hello world");
    }

    #[test]
    fn test_parse_invalid_base64() {
        let input = Value::String("SGVsbG8gd29ybGQ@".to_string());
        let parsed = <Base64 as ScalarType>::parse(input);
        assert!(matches!(parsed, Err(InputValueError { .. })), "{parsed:?}");
    }

    #[test]
    fn test_parse_invalid_boolean_value() {
        let input = Value::Boolean(true);
        let parsed = <Base64 as ScalarType>::parse(input);
        assert!(matches!(parsed, Err(InputValueError { .. })), "{parsed:?}");
    }

    #[test]
    fn test_parse_invalid_number() {
        let input = Value::Number(1.into());
        let parsed = <Base64 as ScalarType>::parse(input);
        assert!(matches!(parsed, Err(InputValueError { .. })), "{parsed:?}");
    }

    #[test]
    fn test_to_value() {
        let base64 = Base64(b"Hello world".to_vec());
        let value = <Base64 as ScalarType>::to_value(&base64);
        assert_eq!(value, Value::String("SGVsbG8gd29ybGQ=".to_string()));
    }

    #[test]
    fn test_from_str_valid() {
        let base64_str = "SGVsbG8gd29ybGQ=";
        let base64 = Base64::from_str(base64_str).unwrap();
        assert_eq!(base64.0, b"Hello world");
    }

    #[test]
    fn test_from_str_invalid() {
        let base64_str = "SGVsbG8gd29ybGQ@";
        let parsed = Base64::from_str(base64_str);
        assert!(matches!(parsed, Err(InputValueError { .. })), "{parsed:?}");
    }

    #[test]
    fn test_from_vec_reference() {
        let vec = vec![1, 2, 3, 4, 5];
        let base64 = Base64::from(&vec);
        assert_eq!(base64.0, vec);
    }

    #[test]
    fn test_from_vec() {
        let vec = vec![1, 2, 3, 4, 5];
        let base64 = Base64::from(vec.clone());
        assert_eq!(base64.0, vec);
    }
}
