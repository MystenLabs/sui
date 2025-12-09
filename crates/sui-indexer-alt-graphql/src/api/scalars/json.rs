// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::{Description, InputValueResult, Scalar, ScalarType, Value};

use crate::error::RpcError;

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

impl TryFrom<serde_json::Value> for Json {
    type Error = RpcError;

    fn try_from(value: serde_json::Value) -> Result<Self, RpcError> {
        Ok(Self(value.try_into().context("Bad JSON")?))
    }
}

impl TryInto<serde_json::Value> for Json {
    type Error = serde_json::Error;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        self.0.into_json()
    }
}
