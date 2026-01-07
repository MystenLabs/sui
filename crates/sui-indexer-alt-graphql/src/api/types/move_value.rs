// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use async_graphql::{Context, Name, Object, Value, dataloader::DataLoader, indexmap::IndexMap};
use move_core_types::{annotated_value as A, annotated_visitor as AV};
use sui_indexer_alt_reader::{displays::DisplayKey, pg_reader::PgReader};
use sui_types::{
    TypeTag,
    display::DisplayVersionUpdatedEvent,
    object::{option_visitor as OV, rpc_visitor as RV},
};
use tokio::join;

use crate::{
    api::scalars::{base64::Base64, json::Json},
    config::Limits,
    error::{RpcError, resource_exhausted},
};

use super::{display::Display, move_type::MoveType};

#[derive(Clone)]
pub(crate) struct MoveValue {
    pub(crate) type_: MoveType,
    pub(crate) native: Vec<u8>,
}

struct JsonVisitor {
    size_budget: usize,
    depth_budget: usize,
}

struct JsonWriter<'b> {
    size_budget: &'b mut usize,
    depth_budget: usize,
}

#[derive(thiserror::Error, Debug)]
enum VisitorError {
    #[error(transparent)]
    Visitor(#[from] AV::Error),

    #[error("Unexpected type")]
    UnexpectedType,

    #[error("Value too big")]
    TooBig,

    #[error("Value too deep")]
    TooDeep,
}

#[Object]
impl MoveValue {
    /// The BCS representation of this value, Base64-encoded.
    async fn bcs(&self) -> Option<Base64> {
        Some(Base64::from(self.native.clone()))
    }

    /// A rendered JSON blob based on an on-chain template, substituted with data from this value.
    ///
    /// Returns `null` if the value's type does not have an associated `Display` template.
    async fn display(&self, ctx: &Context<'_>) -> Option<Result<Display, RpcError>> {
        async {
            let limits: &Limits = ctx.data()?;
            let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

            let Some(TypeTag::Struct(type_)) = self.type_.to_type_tag() else {
                return Ok(None);
            };

            let (layout, display) = join!(
                self.type_.layout_impl(),
                pg_loader.load_one(DisplayKey(*type_)),
            );

            let (Some(layout), Some(display)) =
                (layout?, display.context("Failed to fetch Display")?)
            else {
                return Ok(None);
            };

            let event: DisplayVersionUpdatedEvent = bcs::from_bytes(&display.display)
                .context("Failed to deserialize DisplayVersionUpdatedEvent")?;

            let mut output = IndexMap::new();
            let mut errors = IndexMap::new();

            for (field, value) in
                sui_display::v1::Format::parse(limits.max_display_field_depth, &event.fields)
                    .map_err(resource_exhausted)?
                    .display(limits.max_display_output_size, &self.native, &layout)
                    .map_err(resource_exhausted)?
            {
                match value {
                    Ok(v) => {
                        output.insert(Name::new(&field), Value::String(v));
                    }

                    Err(e) => {
                        output.insert(Name::new(&field), Value::Null);
                        errors.insert(Name::new(&field), Value::String(e.to_string()));
                    }
                };
            }

            Ok(Some(Display {
                output: (!output.is_empty()).then(|| Json::from(Value::from(output))),
                errors: (!errors.is_empty()).then(|| Json::from(Value::from(errors))),
            }))
        }
        .await
        .transpose()
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
    async fn json(&self, ctx: &Context<'_>) -> Option<Result<Json, RpcError>> {
        async {
            let limits: &Limits = ctx.data()?;

            let Some(layout) = self.type_.layout_impl().await? else {
                return Ok(None);
            };

            let value = JsonVisitor::new(limits)
                .deserialize_value(&self.native, &layout)
                .map_err(|e| match &e {
                    VisitorError::Visitor(_) | VisitorError::UnexpectedType => anyhow!(e).into(),
                    VisitorError::TooBig | VisitorError::TooDeep => resource_exhausted(e),
                })?;

            Ok(Some(Json::try_from(value)?))
        }
        .await
        .transpose()
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

impl JsonVisitor {
    fn new(limits: &Limits) -> Self {
        Self {
            size_budget: limits.max_move_value_bound,
            depth_budget: limits.max_move_value_depth,
        }
    }

    fn deserialize_value(
        &mut self,
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
    ) -> Result<serde_json::Value, VisitorError> {
        A::MoveValue::visit_deserialize(
            bytes,
            layout,
            &mut RV::RpcVisitor::new(JsonWriter {
                size_budget: &mut self.size_budget,
                depth_budget: self.depth_budget,
            }),
        )
    }
}

impl JsonWriter<'_> {
    fn debit(&mut self, size: usize) -> Result<(), VisitorError> {
        if *self.size_budget < size {
            return Err(VisitorError::TooBig);
        }

        *self.size_budget -= size;
        Ok(())
    }
}

impl RV::Writer for JsonWriter<'_> {
    type Value = serde_json::Value;
    type Error = VisitorError;

    type Vec = Vec<serde_json::Value>;
    type Map = serde_json::Map<String, serde_json::Value>;

    type Nested<'b>
        = JsonWriter<'b>
    where
        Self: 'b;

    fn nest(&mut self) -> Result<Self::Nested<'_>, Self::Error> {
        if self.depth_budget == 0 {
            return Err(VisitorError::TooDeep);
        }

        Ok(JsonWriter {
            size_budget: self.size_budget,
            depth_budget: self.depth_budget - 1,
        })
    }

    fn write_null(&mut self) -> Result<Self::Value, Self::Error> {
        self.debit("null".len())?;
        Ok(serde_json::Value::Null)
    }

    fn write_bool(&mut self, value: bool) -> Result<Self::Value, Self::Error> {
        self.debit(if value { "true".len() } else { "false".len() })?;
        Ok(serde_json::Value::Bool(value))
    }

    fn write_number(&mut self, value: u32) -> Result<Self::Value, Self::Error> {
        self.debit(if value == 0 { 1 } else { value.ilog10() } as usize)?;
        Ok(serde_json::Value::Number(value.into()))
    }

    fn write_str(&mut self, value: String) -> Result<Self::Value, Self::Error> {
        // Account for the quotes around the string.
        self.debit(2 + value.len())?;
        Ok(serde_json::Value::String(value))
    }

    fn write_vec(&mut self, value: Self::Vec) -> Result<Self::Value, Self::Error> {
        // Account for the opening bracket.
        self.debit(1)?;
        Ok(serde_json::Value::Array(value))
    }

    fn write_map(&mut self, value: Self::Map) -> Result<Self::Value, Self::Error> {
        // Account for the opening brace.
        self.debit(1)?;
        Ok(serde_json::Value::Object(value))
    }

    fn vec_push_element(
        &mut self,
        vec: &mut Self::Vec,
        val: Self::Value,
    ) -> Result<(), Self::Error> {
        // Account for comma (or closing bracket).
        self.debit(1)?;
        vec.push(val);
        Ok(())
    }

    fn map_push_field(
        &mut self,
        map: &mut Self::Map,
        key: String,
        val: Self::Value,
    ) -> Result<(), Self::Error> {
        // Account for quotes, colon, and comma (or closing brace).
        self.debit(4 + key.len())?;
        map.insert(key, val);
        Ok(())
    }
}

impl From<OV::Error> for VisitorError {
    fn from(OV::Error: OV::Error) -> Self {
        VisitorError::UnexpectedType
    }
}

impl From<RV::Error> for VisitorError {
    fn from(RV::Error: RV::Error) -> Self {
        VisitorError::UnexpectedType
    }
}
