// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::anyhow;
use anyhow::bail;

use forking_data_store::Node;
use forking_data_store::stores::GraphQLStore;
use sui_types::base_types::ObjectID;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;

use forking_data_store::CheckpointStore;
use forking_data_store::ObjectKey;
use forking_data_store::ObjectStore;
use sui_types::messages_checkpoint::VerifiedCheckpoint;

/// Directory name appended to the configured filesystem store root.
pub const DATA_STORE_DIR: &str = ".forking_data_store";
/// Per-chain object storage directory.
pub const OBJECTS_DIR: &str = "objects";
/// Per-chain transaction storage directory.
pub const TRANSACTION_DIR: &str = "transaction";
/// Per-chain epoch storage directory.
pub const EPOCH_DIR: &str = "epoch";
/// Per-chain checkpoint storage directory.
pub const CHECKPOINT_DIR: &str = "checkpoint";
/// Marker file for the latest checkpoint sequence known to the store.
pub const LATEST_FILE: &str = "latest";

// `simulacrum` store adapter over a historical fork source.
pub struct DataStore {
    forked_at_checkpoint: CheckpointSequenceNumber,
    node: Node,
    gql: GraphQLStore,
}

impl DataStore {
    pub(crate) async fn new(
        node: Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
        version: &str,
    ) -> Result<Self, anyhow::Error> {
        let gql = GraphQLClient::new(node.clone(), version)?;
        let local = FilesystemStore::new(&node, forked_at_checkpoint)?;

        Ok(Self {
            forked_at_checkpoint,
            gql,
            local,
        })
    }

    fn forked_at_checkpoint(&self) -> CheckpointSequenceNumber {
        self.forked_at_checkpoint
    }

    /// Get a verified checkpoint from remote rpc. If `checkpoint` is `None`, use the store's forked
    /// checkpoint as the default.
    pub(crate) async fn get_verified_checkpoint_from_rpc(
        &self,
        checkpoint: Option<CheckpointSequenceNumber>,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        let checkpoint = checkpoint.unwrap_or(self.forked_at_checkpoint);
        let verified_checkpoint = self.gql.get_verified_checkpoint(Some(checkpoint))?;

        Ok(verified_checkpoint)
    }

    /// Get the object at the latest version available on disk. If not found, it will fetch the
    /// object at the forked checkpoint from remote rpc and save it to disk for future use. Returns
    /// `None` in the latter case.
    pub(crate) fn get_object(&self, object_id: ObjectID) -> anyhow::Result<Option<Object>> {
        self.get_latest_object(object_id)
    }

    /// Get the object at the specified version. It will first try to load from disk, and if not
    /// found, it will fetch from remote rpc by making a query to fetch this version at the forked
    pub(crate) fn get_object_at_version(
        &self,
        object_id: ObjectID,
        version: u64,
    ) -> anyhow::Result<Option<Object>> {
        if let Some(object) = self.local.get_object_at_version(object_id, version)? {
            return Ok(Some(object));
        }

        let object =
            self.get_object_from_remote(object_id, Some(version), self.forked_at_checkpoint())?;

        if let Some(ref object) = object {
            self.local.write_object(object)?;
        }

        Ok(object)
    }

    /// Get the object at the latest version available on disk. If not found, it will fetch the
    /// object at the forked checkpoint from remote rpc and save it to disk for future use. Returns
    /// `None` in the latter case.
    fn get_latest_object(&self, object_id: ObjectID) -> anyhow::Result<Option<Object>> {
        if let Some(object) = self.local.get_latest_object(object_id)? {
            return Ok(Some(object));
        }

        // if not found, load from remote rpc at forked checkpoint and save it to disk for future
        // use
        let object = self.get_object_from_remote(object_id, None, self.forked_at_checkpoint())?;

        if let Some(ref object) = object {
            self.local.write_object(object)?;
        }

        Ok(object)
    }

    /// Get the object at the specified checkpoint from remote rpc. If version is `None`, latest
    /// version at that checkpoint will be returned. Otherwise, the object at the specified version
    /// will be returned if it existed at that checkpoint.
    fn get_object_from_remote(
        &self,
        object_id: ObjectID,
        version: Option<u64>,
        checkpoint: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<Object>> {
        let version_query = if let Some(version) = version {
            VersionQuery::VersionAtCheckpoint {
                version,
                checkpoint,
            }
        } else {
            VersionQuery::AtCheckpoint(checkpoint)
        };

        let objects = self.gql.get_objects(&[ObjectKey {
            object_id,
            version_query,
        }])?;

        Ok(objects
            .into_iter()
            .next()
            .flatten()
            .map(|(object, _)| object))
    }

    /// Get the highest checkpoint sequence number available on disk.
    pub(crate) fn get_highest_checkpoint(&self) -> anyhow::Result<CheckpointSequenceNumber> {
        self.local.get_highest_checkpoint()
    }
}
