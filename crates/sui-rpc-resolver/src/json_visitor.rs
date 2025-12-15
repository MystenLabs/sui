// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A visitor implementation that constructs JSON values from BCS bytes.
//!
//! This visitor traverses BCS-encoded Move data and builds a `serde_json::Value`
//! representation. Note that this approach loads the entire JSON structure into
//! memory, which may have significant memory implications for large objects or
//! collections. It should not be used in memory-constrained contexts like RPC
//! handlers where the size of the data is unbounded.

use move_core_types::{
    annotated_value::{MoveStruct, MoveTypeLayout, MoveValue},
    annotated_visitor as AV,
    language_storage::TypeTag,
};
use serde_json::{Map, Value};
use sui_package_resolver::{PackageStore, Resolver, error::Error as ResolverError};
use sui_types::{
    event::Event,
    object::{option_visitor as OV, rpc_visitor as RV},
};

/// Error type for JSON visitor operations
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Unexpected type")]
    UnexpectedType,

    #[error(transparent)]
    Visitor(#[from] AV::Error),
}

/// Error type for deserialization operations that involve both type resolution and BCS deserialization.
#[derive(thiserror::Error, Debug)]
pub enum DeserializationError {
    /// Failed to fetch type layout from the package resolver.
    #[error("Failed to fetch type layout: {0}")]
    LayoutFetch(#[from] ResolverError),

    /// Failed to deserialize BCS data to JSON.
    #[error("Failed to deserialize BCS data: {0}")]
    Deserialization(#[from] anyhow::Error),
}

/// A visitor that constructs JSON values from BCS bytes.
///
/// Number representation:
/// - u8, u16, u32 are represented as JSON numbers
/// - u64, u128, u256 are represented as strings to avoid precision loss
///
/// Special types:
/// - Addresses use full 64-character hex format with "0x" prefix
/// - Byte vectors (`Vec<u8>`) are Base64-encoded strings
pub struct JsonVisitor;

struct JsonWriter;

impl JsonVisitor {
    /// Deserialize BCS bytes as JSON using the provided type layout.
    pub fn deserialize_value(bytes: &[u8], layout: &MoveTypeLayout) -> anyhow::Result<Value> {
        let mut visitor = RV::RpcVisitor::new(JsonWriter);
        Ok(MoveValue::visit_deserialize(bytes, layout, &mut visitor)?)
    }

    /// Deserialize BCS bytes as a JSON object representing a struct.
    pub fn deserialize_struct(
        bytes: &[u8],
        layout: &move_core_types::annotated_value::MoveStructLayout,
    ) -> anyhow::Result<Value> {
        let mut visitor = RV::RpcVisitor::new(JsonWriter);
        Ok(MoveStruct::visit_deserialize(bytes, layout, &mut visitor)?)
    }

    /// Deserialize a single event to JSON using type resolution.
    ///
    /// This function:
    /// 1. Resolves the type layout for the event's type
    /// 2. Deserializes the BCS-encoded event contents to JSON
    ///
    /// If you need to deserialize multiple events, use
    /// [`deserialize_events`](Self::deserialize_events) instead, which processes
    /// events concurrently for better performance.
    pub async fn deserialize_event<S>(
        event: &Event,
        resolver: &Resolver<S>,
    ) -> Result<Value, DeserializationError>
    where
        S: PackageStore,
    {
        let type_tag = TypeTag::Struct(Box::new(event.type_.clone()));
        let layout = resolver.type_layout(type_tag).await?;
        Ok(Self::deserialize_value(&event.contents, &layout)?)
    }

    /// Deserialize multiple events to JSON concurrently.
    ///
    /// This function processes all events in parallel for better performance.
    ///
    /// If multiple events are from the same package, use
    /// a `Resolver` with a cached `PackageStore` (e.g., `RpcPackageStore::with_cache()`)
    /// to avoid fetching the same package multiple times.
    pub async fn deserialize_events<S>(
        events: &[Event],
        resolver: &Resolver<S>,
    ) -> Result<Vec<Value>, DeserializationError>
    where
        S: PackageStore,
    {
        use futures::future::try_join_all;

        let futures = events
            .iter()
            .map(|event| Self::deserialize_event(event, resolver));
        try_join_all(futures).await
    }
}

impl RV::Writer for JsonWriter {
    type Value = Value;
    type Error = Error;

    type Vec = Vec<Value>;
    type Map = Map<String, Value>;

    type Nested<'a> = Self;

    fn nest(&mut self) -> Result<Self, Error> {
        Ok(Self)
    }

    fn write_null(&mut self) -> Result<Value, Error> {
        Ok(Value::Null)
    }

    fn write_bool(&mut self, value: bool) -> Result<Value, Error> {
        Ok(Value::Bool(value))
    }

    fn write_number(&mut self, value: u32) -> Result<Value, Error> {
        Ok(Value::Number(value.into()))
    }

    fn write_str(&mut self, value: String) -> Result<Value, Error> {
        Ok(Value::String(value))
    }

    fn write_vec(&mut self, value: Self::Vec) -> Result<Value, Error> {
        Ok(Value::Array(value))
    }

    fn write_map(&mut self, value: Self::Map) -> Result<Value, Error> {
        Ok(Value::Object(value))
    }

    fn vec_push_element(&mut self, vec: &mut Vec<Value>, val: Value) -> Result<(), Error> {
        vec.push(val);
        Ok(())
    }

    fn map_push_field(
        &mut self,
        map: &mut Map<String, Value>,
        key: String,
        val: Value,
    ) -> Result<(), Error> {
        map.insert(key, val);
        Ok(())
    }
}

impl From<RV::Error> for Error {
    fn from(RV::Error: RV::Error) -> Self {
        Error::UnexpectedType
    }
}

impl From<OV::Error> for Error {
    fn from(OV::Error: OV::Error) -> Self {
        Error::UnexpectedType
    }
}
