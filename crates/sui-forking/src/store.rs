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
/// Per-chain checkpoint storage directory.
pub const CHECKPOINTS_DIR: &str = "checkpoints";
/// Marker file for the latest checkpoint sequence known to the store.
pub const LATEST_FILE: &str = "latest";

/// A data store for Sui data, with a local filesystem and a remote GraphQL endpoint to query for
/// historical data.
pub struct DataStore {
    forked_at_checkpoint: CheckpointSequenceNumber,
    node: Node,
    gql: GraphQLStore,
}

impl DataStore {
    pub async fn new(
        node: Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
        version: &str,
    ) -> Result<Self, anyhow::Error> {
        let gql = GraphQLStore::new(node.clone(), version)?;

        Ok(Self {
            forked_at_checkpoint,
            node,
            gql,
        })
    }
    /// Return the configured node.
    pub fn node(&self) -> &Node {
        &self.node
    }

    pub fn forked_at_checkpoint(&self) -> CheckpointSequenceNumber {
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
    /// checkpoint.
    pub fn get_object_at_version(
        &self,
        object_id: ObjectID,
        version: u64,
    ) -> anyhow::Result<Object> {
        // load from disk
        if let Some(object) = self.get_object_at_version_from_disk(object_id, version)? {
            return Ok(object);
        }

        let object =
            self.get_object_at_checkpoint_from_remote(object_id, self.forked_at_checkpoint)?;
        self.write_object_to_disk(object.clone())?;

        Ok(object)
    }

    /// Get the object at the latest version available on disk. If not found, it will fetch the
    /// object at the forked checkpoint from remote rpc and save it to disk for future use. Returns
    /// `None` in the latter case.
    fn get_latest_object(&self, object_id: ObjectID) -> anyhow::Result<Option<Object>> {
        // load from disk
        if let Some(object) = self.get_latest_object_from_disk(object_id)? {
            return Ok(Some(object));
        }

        // if not found, load from remote rpc at forked checkpoint and save it to disk for future
        // use
        let object =
            self.get_object_at_checkpoint_from_remote(object_id, self.forked_at_checkpoint)?;
        self.write_object_to_disk(object)?;

        Ok(None)
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
        // find the latest checkpoint on disk
        let checkpoint_dir = self.checkpoints_dir()?;

        anyhow::ensure!(
            checkpoint_dir.exists(),
            "Checkpoint directory does not exist: {}",
            checkpoint_dir.display()
        );

        self.read_latest_file(&checkpoint_dir)
    }

    // *** Filesystem APIs *** //
    // Folder structure:
    // {base_path}/{network_name}/forked_at_{checkpoint}/
    //     - objects/
    //         - {object_id}/
    //            - latest (contains the latest version number)
    //            - {version} (contains the object data in BCS format)
    //      - checkpoints/
    //          - latest (contains the latest checkpoint sequence number)
    //          - {checkpoint} (contains the checkpoint data in BCS format)

    /// Resolve the default base path for on-disk storage.
    pub fn base_path() -> Result<PathBuf, Error> {
        let home_dir = std::env::var("FORKING_DATA_STORE")
            .or_else(|_| std::env::var("SUI_CONFIG_DIR"))
            .or_else(|_| std::env::var("HOME"))
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| {
                anyhow!(
                    "Cannot determine home directory. Define a SUI_DATA_STORE environment variable"
                )
            })?;
        Ok(PathBuf::from(home_dir).join(DATA_STORE_DIR))
    }

    /// Get the directory path for the current node and forked checkpoint, which is used to store
    /// all data related to this fork. The path is resolved as
    /// `{base_path}/{network_name}/forked_at_{checkpoint}`.
    fn node_dir(&self) -> Result<PathBuf, Error> {
        Ok(Self::base_path()?
            .join(self.node.network_name())
            .join(format!("forked_at_{}", self.forked_at_checkpoint)))
    }

    /// Return the directory path for storing objects data.
    fn objects_dir(&self) -> Result<PathBuf, Error> {
        Ok(self.node_dir()?.join(OBJECTS_DIR))
    }

    /// Return the directory path for storing checkpoint data.
    fn checkpoints_dir(&self) -> Result<PathBuf, Error> {
        Ok(self.node_dir()?.join(CHECKPOINTS_DIR))
    }

    /// Get the latest object version available on disk for the given object ID.
    fn get_latest_object_from_disk(&self, object_id: ObjectID) -> anyhow::Result<Option<Object>> {
        let object_dir = self.objects_dir()?.join(object_id.to_string());

        if !object_dir.exists() {
            return Ok(None);
        }

        // find the latest version file in the object directory
        let latest_version = self.read_latest_file(&object_dir)?;
        let version_file = object_dir.join(latest_version.to_string());
        self.read_bcs_file(&version_file)
    }

    fn get_object_at_version_from_disk(
        &self,
        object_id: ObjectID,
        version: u64,
    ) -> anyhow::Result<Option<Object>> {
        let object_dir = self.objects_dir()?.join(object_id.to_string());
        let version_file = object_dir.join(version.to_string());

        if !version_file.exists() {
            return Ok(None);
        }

        self.read_bcs_file(&version_file).map(Some)
    }

    fn read_bcs_file<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<T, Error> {
        let bytes =
            fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))?;
        bcs::from_bytes(&bytes)
            .with_context(|| format!("Failed to deserialize BCS data from: {}", path.display()))
    }

    fn write_bcs_file<T: serde::Serialize>(&self, path: &Path, data: &T) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        let bytes = bcs::to_bytes(data)
            .with_context(|| format!("Failed to serialize BCS data for: {}", path.display()))?;
        fs::write(path, bytes)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        Ok(())
    }

    // ** Objects API ** //

    /// Write the given object to disk under the objects directory, using the object ID and version
    /// as the path. It will also update the latest file to point to this version.
    fn write_object_to_disk(&self, object: Object) -> anyhow::Result<()> {
        let object_dir = self.objects_dir()?.join(object.id().to_string());
        let version_file = object_dir.join(object.version().to_string());
        self.write_bcs_file(&version_file, &object)?;

        // update the latest file with the new version
        let latest_file = object_dir.join(LATEST_FILE);
        fs::write(latest_file, object.version().to_string())
            .with_context(|| format!("Failed to write latest file for object {}", object.id()))
    }

    /// Read the latest file that contains a number representing the latest checkpoint sequence or
    /// object version.
    fn read_latest_file(&self, dir: &Path) -> Result<u64, Error> {
        let latest_path = dir.join(LATEST_FILE);
        if !latest_path.exists() {
            bail!("Latest file not found in directory: {}", dir.display());
        }
        let content = fs::read_to_string(&latest_path)
            .with_context(|| format!("Failed to read latest file: {}", latest_path.display()))?;
        content.trim().parse::<u64>().with_context(|| {
            format!(
                "Failed to parse checkpoint sequence number from latest file: {}",
                latest_path.display()
            )
        })
    }
}
