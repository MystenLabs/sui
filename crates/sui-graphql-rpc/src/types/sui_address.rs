// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use serde::{Deserialize, Serialize};

const SUI_ADDRESS_LENGTH: usize = 32;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Copy)]
pub(crate) struct SuiAddress([u8; SUI_ADDRESS_LENGTH]);
// TODO: unit tests
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

                let bytes = hex::decode(s)?;
                if bytes.len() != SUI_ADDRESS_LENGTH {
                    return Err(InputValueError::custom(format!(
                        "Expected SuiAddress of length {}, received {}.",
                        SUI_ADDRESS_LENGTH,
                        bytes.len()
                    )));
                }
                let mut arr = [0u8; SUI_ADDRESS_LENGTH];
                arr.copy_from_slice(&bytes);
                Ok(SuiAddress(arr))
            }
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(hex::encode(self.0))
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
