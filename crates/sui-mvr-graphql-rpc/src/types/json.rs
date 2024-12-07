// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use async_graphql::*;

/// Arbitrary JSON data.
#[derive(Debug)]
pub(crate) struct Json(Value);

#[Scalar(name = "JSON", use_type_description = true)]
impl ScalarType for Json {
    fn parse(value: Value) -> InputValueResult<Self> {
        Ok(Self(value))
    }

    fn to_value(&self) -> Value {
        self.0.clone()
    }
}

impl Description for Json {
    fn description() -> &'static str {
        "Arbitrary JSON data."
    }
}

impl From<Value> for Json {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
