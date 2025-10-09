// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Helper utilities for converting Event BCS data to JSON.

use move_core_types::language_storage::TypeTag;
use serde_json::Value;
use sui_package_resolver::{PackageStore, Resolver};
use sui_types::event::Event;
use sui_types::object::json_visitor::JsonVisitor;

/// Errors that can occur during Move type resolution and deserialization
#[derive(Debug, thiserror::Error)]
pub enum TypeResolutionError {
    /// Failed to fetch the type layout from the package resolver
    #[error("Failed to fetch type layout: {0}")]
    LayoutFetch(#[from] sui_package_resolver::error::Error),

    /// Failed to deserialize BCS bytes to the target format
    #[error("Failed to deserialize BCS data: {0}")]
    Deserialization(#[from] anyhow::Error),
}

/// Deserialize an Event's BCS contents to JSON.
///
/// This function performs two steps:
/// 1. Fetches the type layout from the package resolver (may fail with `LayoutFetch`)
/// 2. Deserializes the BCS bytes to JSON using the layout (may fail with `Deserialization`)
///
/// # Performance Note
/// When processing multiple events from the same packages, use a PackageStore with
/// caching enabled (e.g., `PackageStoreWithLruCache`) to avoid redundant package fetches.
pub async fn deserialize_event_to_json<S>(
    event: &Event,
    resolver: &Resolver<S>,
) -> Result<Value, TypeResolutionError>
where
    S: PackageStore,
{
    // Resolve the type layout
    let type_tag = TypeTag::Struct(Box::new(event.type_.clone()));
    let layout = resolver.type_layout(type_tag).await?;

    // Deserialize the BCS bytes to JSON
    Ok(JsonVisitor::deserialize_value(&event.contents, &layout)?)
}
