// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Helper utilities for deserializing Event and Object BCS data to JSON.
//!
//! These convenience functions combine type resolution and JSON deserialization,
//! providing an easy-to-use interface for the indexing framework.

use serde_json::Value;
use sui_package_resolver::{PackageStore, Resolver};
use sui_types::event::Event;
use sui_types::object::json_visitor::JsonVisitor;
use sui_types::TypeTag;

/// Errors that can occur during type resolution and JSON deserialization
#[derive(Debug, thiserror::Error)]
pub enum DeserializationError {
    /// Failed to fetch the type layout from the package resolver
    #[error("Failed to fetch type layout: {0}")]
    LayoutFetch(#[from] sui_package_resolver::error::Error),

    /// Failed to deserialize BCS bytes to JSON
    #[error("Failed to deserialize BCS data: {0}")]
    Deserialization(#[from] anyhow::Error),
}

/// Deserialize a single Event's BCS contents to JSON.
///
/// This function performs two steps:
/// 1. Fetches the type layout from the package resolver
/// 2. Deserializes the BCS bytes to JSON using the layout
///
/// # Performance Warning
/// When processing multiple events, use `deserialize_events` instead for better performance
/// through concurrent processing.
///
/// When processing events from the same packages, use a PackageStore with
/// caching enabled (e.g., `PackageStoreWithLruCache`) to avoid redundant package fetches.
pub async fn deserialize_event<S>(
    event: &Event,
    resolver: &Resolver<S>,
) -> Result<Value, DeserializationError>
where
    S: PackageStore,
{
    // Resolve the type layout
    let type_tag = TypeTag::Struct(Box::new(event.type_.clone()));
    let layout = resolver.type_layout(type_tag).await?;

    // Deserialize the BCS bytes to JSON
    Ok(JsonVisitor::deserialize_value(&event.contents, &layout)?)
}

/// Deserialize multiple Events' BCS contents to JSON concurrently.
///
/// Events are processed in parallel for better performance.
/// When processing many events from the same packages, use a PackageStore with
/// caching enabled (e.g., `PackageStoreWithLruCache`) to avoid redundant package fetches.
pub async fn deserialize_events<S>(
    events: &[Event],
    resolver: &Resolver<S>,
) -> Result<Vec<Value>, DeserializationError>
where
    S: PackageStore,
{
    use futures::future::try_join_all;

    let futures = events.iter().map(|event| deserialize_event(event, resolver));
    try_join_all(futures).await
}
