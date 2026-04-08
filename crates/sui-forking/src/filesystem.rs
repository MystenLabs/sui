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
use sui_types::base_types::ObjectID;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;

/// Directory name appended to the configured filesystem store root.
pub const DATA_STORE_DIR: &str = ".forking_data_store";
/// Per-chain object storage directory.
pub const OBJECTS_DIR: &str = "objects";
/// Per-chain checkpoint storage directory.
pub const CHECKPOINTS_DIR: &str = "checkpoints";
/// Marker file for the latest checkpoint sequence known to the store.
pub const LATEST_FILE: &str = "latest";

// Folder structure:
// {base_path}/{network_name}/forked_at_{checkpoint}/
//     - objects/
//         - {object_id}/
//            - latest (contains the latest version number)
//            - {version} (contains the object data in BCS format)
//      - checkpoints/
//          - latest (contains the latest checkpoint sequence number)
//          - {checkpoint} (contains the checkpoint data in BCS format)

/// Local filesystem-backed store for Sui data.
pub struct FilesystemStore {
    node: Node,
    forked_at_checkpoint: CheckpointSequenceNumber,
}

impl FilesystemStore {
    pub fn new(node: Node, forked_at_checkpoint: CheckpointSequenceNumber) -> Self {
        Self {
            node,
            forked_at_checkpoint,
        }
    }

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
    pub fn get_latest_object(&self, object_id: ObjectID) -> anyhow::Result<Option<Object>> {
        let object_dir = self.objects_dir()?.join(object_id.to_string());

        if !object_dir.exists() {
            return Ok(None);
        }

        let latest_version = self.read_latest_file(&object_dir)?;
        let version_file = object_dir.join(latest_version.to_string());
        self.read_bcs_file(&version_file).map(Some)
    }

    pub fn get_object_at_version(
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

    /// Write the given object to disk under the objects directory, using the object ID and version
    /// as the path. It will also update the latest file to point to this version.
    pub fn write_object(&self, object: &Object) -> anyhow::Result<()> {
        let object_dir = self.objects_dir()?.join(object.id().to_string());
        let version_file = object_dir.join(object.version().to_string());
        self.write_bcs_file(&version_file, object)?;

        let latest_file = object_dir.join(LATEST_FILE);
        fs::write(latest_file, object.version().to_string())
            .with_context(|| format!("Failed to write latest file for object {}", object.id()))
    }

    /// Get the highest checkpoint sequence number available on disk.
    pub fn get_highest_checkpoint(&self) -> anyhow::Result<CheckpointSequenceNumber> {
        let checkpoint_dir = self.checkpoints_dir()?;

        anyhow::ensure!(
            checkpoint_dir.exists(),
            "Checkpoint directory does not exist: {}",
            checkpoint_dir.display()
        );

        self.read_latest_file(&checkpoint_dir)
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
