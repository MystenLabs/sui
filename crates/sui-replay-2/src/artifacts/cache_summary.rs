// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use sui_types::{base_types::ObjectID, object::Object};

/// Detailed information about a Move package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub published_id: ObjectID,
    pub original_id: ObjectID,
    pub module_names: Vec<String>,
}

/// Type of object in the replay cache with detailed Move information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectType {
    /// A Move package containing compiled modules
    Package(PackageInfo),
    /// A Move object with its struct tag
    MoveObject(StructTag),
}

/// Entry in the replay cache summary representing an object accessed during replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectCacheEntry {
    pub object_id: ObjectID,
    pub version: u64,
    pub object_type: ObjectType,
}

/// Compact representation of the replay cache for serialization.
/// Contains the execution context (epoch_id from transaction effects, checkpoint from transaction info)
/// and a list of all objects accessed during replay, with their type information but without object content.
/// The epoch_id represents the epoch in which the original transaction was executed.
/// The checkpoint represents the checkpoint sequence number where the transaction was included.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayCacheSummary {
    pub epoch_id: u64,
    pub checkpoint: u64,
    pub network: String,
    pub protocol_version: u64,
    /// List of objects accessed during replay with their version and type information
    pub cache_entries: Vec<ObjectCacheEntry>,
}

impl ReplayCacheSummary {
    /// Create a ReplayCacheSummary from the object cache, extracting detailed Move information.
    pub fn from_cache(
        epoch_id: u64,
        checkpoint: u64,
        network: String,
        protocol_version: u64,
        object_cache: &BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    ) -> Self {
        let mut cache_entries = Vec::new();

        for (object_id, versions) in object_cache {
            for (version, object) in versions {
                let object_type = if object.is_package() {
                    // Extract package information
                    let package = object
                        .data
                        .try_as_package()
                        .expect("Package object should have package data");
                    let package_info = PackageInfo {
                        published_id: package.id(),
                        original_id: package.original_package_id(),
                        module_names: package.serialized_module_map().keys().cloned().collect(),
                    };
                    ObjectType::Package(package_info)
                } else {
                    // Extract Move object struct tag
                    let struct_tag = object
                        .struct_tag()
                        .expect("Move object should have struct tag");
                    ObjectType::MoveObject(struct_tag)
                };

                cache_entries.push(ObjectCacheEntry {
                    object_id: *object_id,
                    version: *version,
                    object_type,
                });
            }
        }

        Self {
            epoch_id,
            checkpoint,
            network,
            protocol_version,
            cache_entries,
        }
    }
}
