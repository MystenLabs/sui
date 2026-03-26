// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! File-system backed store skeleton.
//!
//!

use crate::{
    node::Node, CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore, EpochStoreWriter,
    FullCheckpointData, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, StoreSummary,
    TransactionInfo, TransactionStore, TransactionStoreWriter,
};
use anyhow::{anyhow, Error, Result};
use std::{io::Write, path::PathBuf};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::ProtocolConfig,
};

pub const DATA_STORE_DIR: &str = ".sui_data_store";
pub const NODE_MAPPING_FILE: &str = "node_mapping.csv";
pub const OBJECTS_DIR: &str = "objects";
pub const TRANSACTION_DIR: &str = "transaction";
pub const EPOCH_DIR: &str = "epoch";
pub const CHECKPOINT_DIR: &str = "checkpoint";
pub const CHECKPOINT_FILE_EXTENSION: &str = "binpb.zst";
pub const CHECKPOINT_DIGEST_INDEX_FILE: &str = "checkpoint_digest_index.csv";
pub const CHECKPOINT_CONTENTS_DIGEST_INDEX_FILE: &str = "contents_digest_index.csv";
pub const CHECKPOINT_LATEST_FILE: &str = "latest";
pub const ROOT_VERSIONS_FILE: &str = "root_versions";
pub const CHECKPOINT_VERSIONS_FILE: &str = "checkpoint_versions";

/// Persistent file-system store.
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
        let home_dir = std::env::var("SUI_DATA_STORE")
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
    ) -> Result<Option<FullCheckpointData>, Error> {
        todo!("filesystem checkpoint reads are not implemented in the skeleton")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<FullCheckpointData>, Error> {
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
    fn write_checkpoint(&self, _checkpoint: &FullCheckpointData) -> Result<(), Error> {
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
