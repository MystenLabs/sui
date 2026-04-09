// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use forking_data_store::Node;
use forking_data_store::stores::GraphQLStore;
use sui_types::base_types::ObjectID;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;

use forking_data_store::CheckpointStore;
use forking_data_store::ObjectKey;
use forking_data_store::ObjectStore;
use sui_types::messages_checkpoint::VerifiedCheckpoint;

use crate::filesystem::FilesystemStore;

/// A data store for Sui data, with a local filesystem and a remote GraphQL endpoint to query for
/// historical data.
pub struct DataStore {
    forked_at_checkpoint: CheckpointSequenceNumber,
    gql: GraphQLStore,
    local: FilesystemStore,
}

impl DataStore {
    pub async fn new(
        node: Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
        version: &str,
    ) -> Result<Self, anyhow::Error> {
        let gql = GraphQLStore::new(node.clone(), version)?;
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
    pub async fn get_verified_checkpoint_from_rpc(
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
    pub fn get_object(&self, object_id: ObjectID) -> anyhow::Result<Option<Object>> {
        self.get_latest_object(object_id)
    }

    /// Get the object at the specified version. It will first try to load from disk, and if not
    /// found, it will fetch from remote rpc by making a query to fetch this version at the forked
    /// checkpoint. If none is found, it will return None. If the object is successfully fetched
    /// from remote rpc, it will be saved to disk for future use before returning the object.
    pub fn get_object_at_version(
        &self,
        object_id: ObjectID,
        version: u64,
    ) -> anyhow::Result<Option<Object>> {
        if let Some(object) = self.local.get_object_at_version(object_id, version)? {
            return Ok(Some(object));
        }

        let object = self.get_object_at_version_at_checkpoint_from_remote(
            object_id,
            version,
            self.forked_at_checkpoint(),
        )?;
        if let Some(object) = object {
            self.local.write_object(&object)?;
            Ok(Some(object))
        } else {
            Ok(None)
        }
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
        let object =
            self.get_object_at_checkpoint_from_remote(object_id, self.forked_at_checkpoint)?;
        self.local.write_object(&object)?;

        Ok(None)
    }

    /// Get the object at the specified version and specified checkpoint from remote rpc.
    fn get_object_at_version_at_checkpoint_from_remote(
        &self,
        object_id: ObjectID,
        version: u64,
        checkpoint: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<Object>> {
        let objects = self.gql.get_objects(&[ObjectKey {
            object_id,
            version_query: forking_data_store::VersionQuery::VersionAtCheckpoint {
                version,
                checkpoint,
            },
        }])?;

        Ok(objects
            .into_iter()
            .next()
            .flatten()
            .map(|(object, _)| object))
    }

    /// Get the object at the specified checkpoint from remote rpc.
    fn get_object_at_checkpoint_from_remote(
        &self,
        object_id: ObjectID,
        checkpoint: CheckpointSequenceNumber,
    ) -> anyhow::Result<Object> {
        let objects = self.gql.get_objects(&[ObjectKey {
            object_id,
            version_query: forking_data_store::VersionQuery::AtCheckpoint(checkpoint),
        }])?;

        if let Some(Some((object, _))) = objects.into_iter().next() {
            Ok(object)
        } else {
            Err(anyhow!(
                "Object {} not found at checkpoint {}",
                object_id,
                checkpoint
            ))
        }
    }

    /// Get the highest checkpoint sequence number available on disk.
    pub fn get_highest_checkpoint(&self) -> anyhow::Result<CheckpointSequenceNumber> {
        self.local.get_highest_checkpoint()
    }
}
