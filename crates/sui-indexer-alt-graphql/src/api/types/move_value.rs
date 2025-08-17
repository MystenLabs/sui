// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Name, Object};
use prost_types::{self as proto, value::Kind};
use serde_json::Number;
use sui_types::proto_value::ProtoVisitorBuilder;

use crate::{
    api::scalars::{base64::Base64, json::Json},
    config::Limits,
    error::{resource_exhausted, RpcError},
};

use super::move_type::MoveType;

pub(crate) struct MoveValue {
    type_: MoveType,
    native: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
#[error("Move value is too big")]
pub(crate) struct MoveValueTooBigError;

#[Object]
impl MoveValue {
    /// The BCS representation of this value, Base64-encoded.
    async fn bcs(&self) -> Option<Base64> {
        Some(Base64::from(self.native.clone()))
    }

    /// Representation of a Move value in JSON, where:
    ///
    /// - Addresses, IDs, and UIDs are represented in canonical form, as JSON strings.
    /// - Bools are represented by JSON boolean literals.
    /// - u8, u16, and u32 are represented as JSON numbers.
    /// - u64, u128, and u256 are represented as JSON strings.
    /// - Balances, Strings, and Urls are represented as JSON strings.
    /// - Vectors of bytes are represented as Base64 blobs, and other vectors are represented by JSON arrays.
    /// - Structs are represented by JSON objects.
    /// - Enums are represented by JSON objects, with a field named `@variant` containing the variant name.
    /// - Empty optional values are represented by `null`.
    async fn json(&self, ctx: &Context<'_>) -> Result<Option<Json>, RpcError> {
        let limits: &Limits = ctx.data()?;

        let Some(layout) = self.type_.layout_impl().await? else {
            return Ok(None);
        };

        let value = ProtoVisitorBuilder::new(limits.max_move_value_bound)
            .deserialize_value(&self.native, &layout)
            .map_err(|_| resource_exhausted(MoveValueTooBigError))?;

        Ok(Some(Json::from(proto_to_json(value))))
    }

    /// The value's type.
    async fn type_(&self) -> Option<MoveType> {
        Some(self.type_.clone())
    }
}

impl MoveValue {
    pub(crate) fn new(type_: MoveType, native: Vec<u8>) -> Self {
        Self { type_, native }
    }
}

/// Convert a Protobuf value into a GraphQL JSON value.
fn proto_to_json(proto: proto::Value) -> async_graphql::Value {
    match proto.kind {
        Some(Kind::NullValue(_)) | None => async_graphql::Value::Null,
        Some(Kind::BoolValue(b)) => async_graphql::Value::Boolean(b),
        Some(Kind::StringValue(s)) => async_graphql::Value::String(s),

        // The [`ProtoVisitor`] only produces numbers for `u8`, `u16`, and `u32` values, so they
        // can be encoded as a whole number in JSON without loss of precision by conversion to
        // `u32`.
        Some(Kind::NumberValue(n)) => async_graphql::Value::Number(Number::from(n as u32)),

        Some(Kind::StructValue(map)) => async_graphql::Value::Object(
            map.fields
                .into_iter()
                .map(|(k, v)| (Name::new(k), proto_to_json(v)))
                .collect(),
        ),

        Some(Kind::ListValue(list)) => {
            async_graphql::Value::List(list.values.into_iter().map(proto_to_json).collect())
        }
    }
}
