// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(transparent)]
pub(crate) struct Float(pub f64);

#[Scalar(use_type_description = true)]
impl ScalarType for Float {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            // Parse the float value
            Ok(value.parse().map(Float)?)
        } else {
            // If the type does not match
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

impl Description for Float {
    fn description() -> &'static str {
        "String representation of an arbitrary width, double precision number"
    }
}
