// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! File-system backed store skeleton.
//!
//! This module defines the intended on-disk layout for the forking data store.
//! The actual read/write paths are still being implemented, but the constants in
//! this file describe the directory and file names the backend will use.
//!
//! # Base Directory
//!
//! [`FileSystemStore::base_path`] resolves the store root from
//! `FORKING_DATA_STORE` first, then `SUI_CONFIG_DIR`, `HOME`, or
//! `USERPROFILE`, and finally appends [`DATA_STORE_DIR`].
//!
//! # Intended Directory Structure
//!
//! ```text
//! ~./<store root>/
//!   node_mapping.csv
//!   <chain_id>/
//!     transaction/
//!       <tx_digest>
//!     epoch/
//!       <epoch_id>
//!     objects/
//!       <object_id>/
//!         <actual_version>
//!         root_versions
//!         checkpoint_versions
//!     checkpoint/
//!       latest
//!       checkpoint_digest_index.csv
//!       contents_digest_index.csv
//!       <sequence>.binpb.zst
//! ```
//!
//! The `checkpoint/` directory is intended to store full checkpoint payloads
//! and the indexes needed to resolve checkpoint and contents digests back to a
//! sequence number.
//!
//! # File Formats
//!
//! - Transaction files in `transaction/<tx_digest>` store `TransactionFileData` as BCS, which
//!   includes the original `TransactionData`, its `TransactionEffects`, and the execution
//!   `checkpoint`.
//! - Epoch files in `epoch/<epoch_id>` store `EpochFileData` as BCS, capturing epoch metadata.
//! - Object files in `objects/<object_id>/<version>` store the `Object` at the corresponding
//!   version as BCS.
//! - Checkpoint files in `checkpoint/<sequence>.binpb.zst` store the `CheckpointData` as a
//!   compressed protobuf payload.
//!
//! # Version Mapping Files
//!
//! The `root_versions` and `checkpoint_versions` files provide stable lookups for queries
//! that are not targeted to a specific version. Each line is a comma-separated pair and the
//! files can be edited manually or updated by this store:
//!
//! - `root_versions`: maps a root version bound to the actual stored version.
//!   - line format: `<max_version>,<actual_version>`
//!   - example: `4,3` means the highest version not exceeding 4 is 3
//!
//! - `checkpoint_versions`: maps a checkpoint to the actual stored version for the object
//!   at that point.
//!   - line format: `<checkpoint>,<actual_version>`
//!   - example: `100,1` means at checkpoint 100 the object version is 1
//!
//! These mapping files allow answering `VersionQuery::RootVersion(_)` and
//! `VersionQuery::AtCheckpoint(_)` without requiring a full index; the store writes them as
//! it learns the concrete versions.

use std::{io::Write, path::PathBuf};

use anyhow::{Error, Result, anyhow};

use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::ProtocolConfig,
};

use crate::{
    CheckpointData, CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore,
    EpochStoreWriter, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, StoreSummary,
    TransactionInfo, TransactionStore, TransactionStoreWriter, node::Node,
};

/// Directory name appended to the configured filesystem store root.
pub const DATA_STORE_DIR: &str = ".forking_data_store";
/// CSV file mapping a logical node name to a concrete chain identifier.
pub const NODE_MAPPING_FILE: &str = "node_mapping.csv";
/// Per-chain object storage directory.
pub const OBJECTS_DIR: &str = "objects";
/// Per-chain transaction storage directory.
pub const TRANSACTION_DIR: &str = "transaction";
/// Per-chain epoch storage directory.
pub const EPOCH_DIR: &str = "epoch";
/// Per-chain checkpoint storage directory.
pub const CHECKPOINT_DIR: &str = "checkpoint";
/// File extension used for serialized checkpoint payloads.
pub const CHECKPOINT_FILE_EXTENSION: &str = "binpb.zst";
/// CSV index mapping checkpoint digests to checkpoint sequence numbers.
pub const CHECKPOINT_DIGEST_INDEX_FILE: &str = "checkpoint_digest_index.csv";
/// CSV index mapping checkpoint contents digests to checkpoint sequence numbers.
pub const CHECKPOINT_CONTENTS_DIGEST_INDEX_FILE: &str = "contents_digest_index.csv";
/// Marker file for the latest checkpoint sequence known to the store.
pub const CHECKPOINT_LATEST_FILE: &str = "latest";
/// Per-object CSV file mapping root-version queries to concrete object versions.
pub const ROOT_VERSIONS_FILE: &str = "root_versions";
/// Per-object CSV file mapping checkpoint queries to concrete object versions.
pub const CHECKPOINT_VERSIONS_FILE: &str = "checkpoint_versions";

/// Persistent file-system store rooted at a configurable on-disk path.
#[derive(Debug)]
pub struct FileSystemStore {
    node: Node,
    base_path: PathBuf,
}

impl FileSystemStore {
    /// Create a store rooted at the default data-store path.
    pub fn new(node: Node) -> Result<Self, Error> {
        let base_path = Self::base_path()?;
        Ok(Self { node, base_path })
    }

    /// Create a store rooted at an explicit path.
    pub fn new_with_path(node: Node, full_path: PathBuf) -> Result<Self, Error> {
        Ok(Self {
            node,
            base_path: full_path,
        })
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

    /// Return the configured node.
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// Return the configured base path.
    pub fn store_path(&self) -> &PathBuf {
        &self.base_path
    }

    /// Filesystem-specific latest-object helper.
    pub fn get_object_latest(&self, _object_id: &ObjectID) -> Result<Option<(Object, u64)>, Error> {
        todo!("filesystem latest-object lookup is not implemented in the skeleton")
    }

    /// Filesystem-specific owner scan helper.
    pub fn get_objects_by_owner(&self, _owner: SuiAddress) -> Result<Vec<Object>, Error> {
        todo!("filesystem owner scan is not implemented in the skeleton")
    }

    /// Filesystem-specific exact-version helper.
    pub fn get_object_at_version(
        &self,
        _object_id: &ObjectID,
        _version: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        todo!("filesystem exact-version lookup is not implemented in the skeleton")
    }

    /// Filesystem-specific root-version helper.
    pub fn get_object_at_root_version(
        &self,
        _object_id: &ObjectID,
        _root_version: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        todo!("filesystem root-version lookup is not implemented in the skeleton")
    }

    /// Filesystem-specific checkpoint helper.
    pub fn get_object_at_checkpoint(
        &self,
        _object_id: &ObjectID,
        _checkpoint: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        todo!("filesystem checkpoint lookup is not implemented in the skeleton")
    }
}

impl TransactionStore for FileSystemStore {
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("filesystem transaction reads are not implemented in the skeleton")
    }
}

impl TransactionStoreWriter for FileSystemStore {
    fn write_transaction(
        &self,
        _tx_digest: &str,
        _transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        todo!("filesystem transaction writes are not implemented in the skeleton")
    }
}

impl EpochStore for FileSystemStore {
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("filesystem epoch reads are not implemented in the skeleton")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("filesystem protocol-config reads are not implemented in the skeleton")
    }
}

impl EpochStoreWriter for FileSystemStore {
    fn write_epoch_info(&self, _epoch: u64, _epoch_data: EpochData) -> Result<(), Error> {
        todo!("filesystem epoch writes are not implemented in the skeleton")
    }
}

impl ObjectStore for FileSystemStore {
    fn get_objects(&self, _keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        todo!("filesystem object reads are not implemented in the skeleton")
    }
}

impl ObjectStoreWriter for FileSystemStore {
    fn write_object(
        &self,
        _key: &ObjectKey,
        _object: Object,
        _actual_version: u64,
    ) -> Result<(), Error> {
        todo!("filesystem object writes are not implemented in the skeleton")
    }
}

impl CheckpointStore for FileSystemStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        todo!("filesystem checkpoint reads are not implemented in the skeleton")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        todo!("filesystem latest-checkpoint lookup is not implemented in the skeleton")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("filesystem checkpoint-digest lookups are not implemented in the skeleton")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("filesystem contents-digest lookups are not implemented in the skeleton")
    }
}

impl CheckpointStoreWriter for FileSystemStore {
    fn write_checkpoint(&self, _checkpoint: &CheckpointData) -> Result<(), Error> {
        todo!("filesystem checkpoint writes are not implemented in the skeleton")
    }
}

impl SetupStore for FileSystemStore {
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        todo!("filesystem setup is not implemented in the skeleton")
    }
}

impl StoreSummary for FileSystemStore {
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(
            writer,
            "FileSystemStore(node={}, base_path={})",
            self.node.network_name(),
            self.base_path.display()
        )?;
        Ok(())
    }
}
