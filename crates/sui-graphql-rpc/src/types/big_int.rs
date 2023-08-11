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
                // check that all are digits
                if s.chars().all(|c| c.is_ascii_digit()) {
                    Ok(BigInt(s))
                } else {
                    Err(InputValueError::custom("Invalid BigInt"))
                }
            }
            _ => Err(InputValueError::custom("Invalid BigInt")),
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
