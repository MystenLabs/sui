// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::{self, Context};
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use sui_indexer_alt_framework::types::base_types::SuiAddress;
use sui_indexer_alt_framework::types::collection_types::VecMap;
use sui_indexer_alt_framework::types::dynamic_field::Field;
use sui_indexer_alt_framework::types::id::UID;
use sui_indexer_alt_framework::types::object::{Object, Owner};

// ============================================================================
// WALRUS BLOB DESERIALIZATION TYPES
// ============================================================================
// These types represent the structure of Walrus blob data as it exists on-chain.
// They are used for deserializing Move objects into Rust structs.

#[derive(Debug, Serialize, Deserialize)]
pub struct Blob {
    pub id: UID,
    pub registered_epoch: u32,
    pub blob_id: BlobId,
    pub size: u64,
    pub encoding_type: u8,
    // Stores the epoch first certified.
    pub certified_epoch: Option<u32>,
    pub storage: StorageResource,
    // Marks if this blob can be deleted.
    pub deletable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageResource {
    pub id: UID,
    pub start_epoch: u32,
    pub end_epoch: u32,
    pub storage_size: u64,
}

/// The ID of a blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
#[repr(transparent)]
pub struct BlobId(pub [u8; 32]);

#[derive(Debug, Serialize, Deserialize)]
pub struct DynamicFieldName(pub Vec<u8>);

#[derive(Debug, Serialize, Deserialize)]
pub struct BlobAttribute {
    pub metadata: VecMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct BlogPostMetadata {
    pub publisher: Vec<u8>,
    pub title: String,
    pub view_count: u64,
}

/// Try to deserialize the object as a Walrus Metadata dynamic field, and return the
/// deserialized data and the parent object ID, or return None if it is not.
pub fn get_metadata(tag: &StructTag, object: &Object) -> anyhow::Result<Option<BlobAttribute>> {
    // Must be a MoveObject
    let Some(type_) = object.type_() else {
        return Ok(None);
    };

    // The expected type of the dynamic field is a `Field<DynamicFieldName, BlobAttribute>`.
    if !type_.is(tag) {
        return Ok(None);
    }

    let move_object = object
        .data
        .try_as_move()
        .ok_or_else(|| anyhow::anyhow!("Not a Move object"))?;

    // This is called during `process`, so the indexing framework can trace the error
    let field: Field<DynamicFieldName, BlobAttribute> =
        bcs::from_bytes(move_object.contents()).context("Failed to deserialize")?;

    Ok(Some(field.value))
}

/// Attempt to extract content from Walrus metadata dynamic field and parent object id, otherwise
/// return None.
pub fn extract_content_from_metadata(
    tag: &StructTag,
    object: &Object,
) -> anyhow::Result<Option<(BlogPostMetadata, SuiAddress)>> {
    let Some(metadata) = get_metadata(tag, object)? else {
        return Ok(None);
    };

    // Dynamic fields must have an ObjectOwner
    let Owner::ObjectOwner(parent_id) = object.owner() else {
        return Ok(None);
    };

    let (Some(title), Some(view_count), Some(publisher)) = (
        metadata.metadata.get(&"title".to_owned()),
        metadata.metadata.get(&"view_count".to_owned()),
        metadata.metadata.get(&"publisher".to_owned()),
    ) else {
        return Err(anyhow::anyhow!("Missing title, view_count, or publisher"));
    };

    let view_count = view_count
        .parse::<u64>()
        .context("Failed to parse view_count")?;

    let publisher = SuiAddress::from_str(publisher)
        .context("Failed to parse publisher")?
        .to_vec();

    let blog_post_metadata = BlogPostMetadata {
        publisher,
        title: title.to_string(),
        view_count,
    };

    Ok(Some((blog_post_metadata, *parent_id)))
}
