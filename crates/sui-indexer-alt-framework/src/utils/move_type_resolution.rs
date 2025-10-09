// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Utilities for resolving Move types from BCS bytes.
//!
//! This module provides helper functions for deserializing BCS-encoded Move data
//! using the PackageResolver to fetch type layouts.

use anyhow::Result;
use move_core_types::annotated_value::MoveValue;
use move_core_types::annotated_visitor::Visitor;
use move_core_types::language_storage::TypeTag;
use serde_json::Value as JsonValue;
use sui_package_resolver::{PackageStore, Resolver};

use crate::utils::json_visitor::JsonVisitor;

/// Errors that can occur during Move type resolution
#[derive(Debug, thiserror::Error)]
pub enum TypeResolutionError {
    #[error("Failed to fetch type layout: {0}")]
    LayoutFetch(#[from] sui_package_resolver::error::Error),

    #[error("Visitor error: {0}")]
    VisitorError(Box<dyn std::error::Error + Send + Sync>),
}

/// Deserializes BCS-encoded Move data by resolving its type and using a visitor pattern.
///
/// BCS (Binary Canonical Serialization) encodes data without type information - it's just
/// raw bytes. To deserialize these bytes, we need to know the data structure, which we
/// get by resolving the type tag.
///
/// # Parameters
///
/// * `bcs_bytes` - Raw binary data to deserialize
/// * `type_tag` - The Move type identifier (e.g., `0x2::coin::Coin<0x2::sui::SUI>`)
/// * `resolver` - Fetches type layouts from the blockchain
/// * `visitor` - Defines how to construct a value while traversing the deserialized data
///
/// # How It Works
///
/// 1. Type Layout Fetch: The resolver fetches the type layout for the given type tag.
///    This layout describes the structure of the data (what fields, their types, etc.).
///
/// 2. Visitor-based Construction: Using the layout as a guide, we traverse the BCS
///    bytes and call the visitor's methods (visit_u64, visit_struct, etc.) for each
///    piece of data. The visitor defines what value to construct - it could build a
///    MoveStruct, convert to JSON, extract specific fields, etc.
///
/// # Example
/// ```ignore
/// let resolver = create_default_resolver(config)?;
/// let mut json_visitor = JsonVisitor::new();  // Constructs JSON values
/// let json = deserialize_typed_bcs(
///     &event.contents,
///     &event.type_,
///     &resolver,
///     &mut json_visitor
/// ).await?;
/// ```
pub async fn deserialize_typed_bcs<S, V, R, E>(
    bcs_bytes: &[u8],
    type_tag: &TypeTag,
    resolver: &Resolver<S>,
    visitor: &mut V,
) -> Result<R, TypeResolutionError>
where
    S: PackageStore,
    V: for<'a, 'b> Visitor<'a, 'b, Value = R, Error = E>,
    E: std::error::Error + Send + Sync + 'static,
{
    let layout = resolver.type_layout(type_tag.clone()).await?;

    MoveValue::visit_deserialize(bcs_bytes, &layout, visitor)
        .map_err(|e| TypeResolutionError::VisitorError(e.into()))
}

/// Convenience function to deserialize BCS bytes directly to JSON.
///
/// This combines type resolution with JSON conversion in a single call,
/// making it easy to convert Move data to JSON format.
///
/// # Example
/// ```ignore
/// let resolver = create_default_resolver(config)?;
/// let json = typed_bcs_to_json(
///     &event.contents,
///     &event.type_,
///     &resolver
/// ).await?;
/// println!("Event data: {}", serde_json::to_string_pretty(&json)?);
/// ```
pub async fn typed_bcs_to_json<S>(
    bcs_bytes: &[u8],
    type_tag: &TypeTag,
    resolver: &Resolver<S>,
) -> Result<JsonValue, TypeResolutionError>
where
    S: PackageStore,
{
    let mut json_visitor = JsonVisitor::new();
    deserialize_typed_bcs(bcs_bytes, type_tag, resolver, &mut json_visitor).await
}
