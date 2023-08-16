// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::*;
use fastcrypto::encoding::Base64 as FastCryptoBase64;
use fastcrypto::encoding::Encoding as FastCryptoEncoding;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Base64(Vec<u8>);

// TODO: implement Base64 scalar type
// TODO: unit tests
#[Scalar]
impl ScalarType for Base64 {
    fn parse(value: Value) -> InputValueResult<Self> {
        // TODO: improve errors
        match value {
            Value::String(s) => {
                Ok(Base64(FastCryptoBase64::decode(&s).map_err(|r| {
                    InputValueError::custom(format!("Invalid Base64: {}", r))
                })?))
            }
            _ => Err(InputValueError::custom(
                "Invalid Base64: Input must be String type",
            )),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(FastCryptoBase64::encode(self.0.clone()))
    }
}

impl FromStr for Base64 {
    type Err = InputValueError<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Base64(
            FastCryptoBase64::decode(s).map_err(|_| InputValueError::custom("Invalid Base64"))?,
        ))
    }
}

impl From<&Vec<u8>> for Base64 {
    fn from(bytes: &Vec<u8>) -> Self {
        Base64(bytes.clone())
    }
}

impl From<Vec<u8>> for Base64 {
    fn from(bytes: Vec<u8>) -> Self {
        Base64::from(&bytes)
    }
}
