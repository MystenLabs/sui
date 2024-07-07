// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use async_graphql::*;
use sui_types::{base_types::SequenceNumber, sui_serde::BigInt};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct UInt(u64);

/// An unsigned integer that can hold values up to 2^53 - 1. This can be treated similarly to `Int`,
/// but it is guaranteed to be non-negative, and it may be larger than 2^31 - 1.
#[Scalar(name = "UInt")]
impl ScalarType for UInt {
    fn parse(value: Value) -> InputValueResult<Self> {
        let Value::Number(n) = value else {
            return Err(InputValueError::expected_type(value));
        };

        let Some(n) = n.as_u64() else {
            return Err(InputValueError::custom("Expected an unsigned integer."));
        };

        Ok(UInt(n))
    }

    fn to_value(&self) -> Value {
        Value::Number(self.0.into())
    }
}

impl fmt::Display for UInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for UInt {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<UInt> for SequenceNumber {
    fn from(value: UInt) -> Self {
        SequenceNumber::from(value.0)
    }
}

impl From<UInt> for BigInt<u64> {
    fn from(value: UInt) -> Self {
        BigInt::from(value.0)
    }
}

impl From<UInt> for u64 {
    fn from(value: UInt) -> Self {
        value.0
    }
}

impl From<UInt> for i64 {
    fn from(value: UInt) -> Self {
        value.0 as i64
    }
}
