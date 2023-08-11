// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::*;
use fastcrypto::encoding::Encoding;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Base64(String);

/// TODO: implement Base64 scalar type
#[Scalar]
impl ScalarType for Base64 {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::String(s) => {
                fastcrypto::encoding::Base64::decode(&s)
                    .map_err(|_| InputValueError::custom("Invalid Base64"))?;
                Ok(Base64(s))
            }
            _ => Err(InputValueError::custom("Invalid Base64")),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.clone())
    }
}

impl FromStr for Base64 {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Base64(s.to_string()))
    }
}

impl From<&Vec<u8>> for Base64 {
    fn from(bytes: &Vec<u8>) -> Self {
        Base64(<fastcrypto::encoding::Base64 as fastcrypto::encoding::Encoding>::encode(bytes))
    }
}
