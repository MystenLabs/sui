// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BigInt(String);

#[Scalar]
impl ScalarType for BigInt {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::String(s) => {
                let mut r = &s[..];
                let mut signed = false;
                // check that all are digits and first can start with -
                if s.starts_with('-') {
                    r = s.strip_prefix('-').unwrap();
                    signed = true;
                }
                r = r.trim_start_matches('0');

                Ok(BigInt(if r.is_empty() {
                    "0".to_string()
                } else if r.chars().all(|c| c.is_ascii_digit()) {
                    format!("{}{}", if signed { "-" } else { "" }, r)
                } else {
                    return Err(InputValueError::custom(
                        "Invalid BigInt value. All characters should be digits.",
                    ));
                }))
            }
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.clone())
    }
}

impl FromStr for BigInt {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(BigInt(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_value() {
        assert_eq!(
            <BigInt as ScalarType>::parse(Value::String("123".to_string())).unwrap(),
            BigInt("123".to_string())
        );
        assert_eq!(
            <BigInt as InputType>::parse(Some(Value::String("123".to_string()))).unwrap(),
            BigInt("123".to_string())
        );

        assert_eq!(
            <BigInt as ScalarType>::parse(Value::String("-123".to_string())).unwrap(),
            BigInt("-123".to_string())
        );
        assert_eq!(
            <BigInt as InputType>::parse(Some(Value::String("-123".to_string()))).unwrap(),
            BigInt("-123".to_string())
        );

        assert_eq!(
            <BigInt as ScalarType>::parse(Value::String("00233".to_string())).unwrap(),
            BigInt("233".to_string())
        );
        assert_eq!(
            <BigInt as InputType>::parse(Some(Value::String("00233".to_string()))).unwrap(),
            BigInt("233".to_string())
        );

        assert_eq!(
            <BigInt as ScalarType>::parse(Value::String("0".to_string())).unwrap(),
            BigInt("0".to_string())
        );
        assert_eq!(
            <BigInt as InputType>::parse(Some(Value::String("0".to_string()))).unwrap(),
            BigInt("0".to_string())
        );

        assert_eq!(
            <BigInt as ScalarType>::parse(Value::String("-0".to_string())).unwrap(),
            BigInt("0".to_string())
        );
        assert_eq!(
            <BigInt as InputType>::parse(Some(Value::String("-0".to_string()))).unwrap(),
            BigInt("0".to_string())
        );

        assert_eq!(
            <BigInt as ScalarType>::parse(Value::String("000".to_string())).unwrap(),
            BigInt("0".to_string())
        );
        assert_eq!(
            <BigInt as InputType>::parse(Some(Value::String("000".to_string()))).unwrap(),
            BigInt("0".to_string())
        );

        assert_eq!(
            <BigInt as ScalarType>::parse(Value::String("-000".to_string())).unwrap(),
            BigInt("0".to_string())
        );
        assert_eq!(
            <BigInt as InputType>::parse(Some(Value::String("-000".to_string()))).unwrap(),
            BigInt("0".to_string())
        );

        assert!(<BigInt as ScalarType>::parse(Value::String("123a".to_string())).is_err());
        assert!(<BigInt as InputType>::parse(Some(Value::String("123a".to_string()))).is_err());

        assert!(<BigInt as ScalarType>::parse(Value::String("a123".to_string())).is_err());
        assert!(<BigInt as InputType>::parse(Some(Value::String("a123".to_string()))).is_err());

        assert!(<BigInt as ScalarType>::parse(Value::String("123-".to_string())).is_err());
        assert!(<BigInt as InputType>::parse(Some(Value::String("123-".to_string()))).is_err());

        assert!(<BigInt as ScalarType>::parse(Value::String(" 123".to_string())).is_err());
        assert!(<BigInt as InputType>::parse(Some(Value::String(" 123".to_string()))).is_err());
    }
}
