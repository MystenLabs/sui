// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use anyhow::{Error, Result};

use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
};

use crate::{
    CheckpointData, CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore,
    EpochStoreWriter, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, StoreSummary,
    TransactionInfo, TransactionStore, TransactionStoreWriter, node::Node,
};

const DEFAULT_TXN_CAP: usize = 10_000;
const DEFAULT_EPOCH_CAP: usize = 10_000;
const DEFAULT_OBJECT_CAP: usize = 50_000;
const DEFAULT_ROOT_MAP_CAP: usize = 50_000;
const DEFAULT_CHECKPOINT_MAP_CAP: usize = 50_000;

/// Bounded in-memory store.
#[derive(Debug)]
pub struct LruMemoryStore {
    node: Node,
    txn_cap: usize,
    epoch_cap: usize,
    object_cap: usize,
    root_map_cap: usize,
    checkpoint_map_cap: usize,
}

impl LruMemoryStore {
    /// Create a new LRU store with default capacities.
    pub fn new(node: Node) -> Self {
        Self::with_capacities(
            node,
            DEFAULT_TXN_CAP,
            DEFAULT_EPOCH_CAP,
            DEFAULT_OBJECT_CAP,
            DEFAULT_ROOT_MAP_CAP,
            DEFAULT_CHECKPOINT_MAP_CAP,
        )
    }

    /// Create a new LRU store with explicit capacities.
    pub fn with_capacities(
        node: Node,
        txn_cap: usize,
        epoch_cap: usize,
        object_cap: usize,
        root_map_cap: usize,
        checkpoint_map_cap: usize,
    ) -> Self {
        Self {
            node,
            txn_cap,
            epoch_cap,
            object_cap,
            root_map_cap,
            checkpoint_map_cap,
        }
    }

    /// Return the chain associated with the configured node.
    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    /// Return the configured node.
    pub fn node(&self) -> &Node {
        &self.node
    }
}

impl TransactionStore for LruMemoryStore {
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("LRU transaction reads are not implemented in the skeleton")
    }
}

impl TransactionStoreWriter for LruMemoryStore {
    fn write_transaction(
        &self,
        _tx_digest: &str,
        _transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        todo!("LRU transaction writes are not implemented in the skeleton")
    }
}

impl EpochStore for LruMemoryStore {
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("LRU epoch reads are not implemented in the skeleton")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("LRU protocol-config reads are not implemented in the skeleton")
    }
}

impl EpochStoreWriter for LruMemoryStore {
    fn write_epoch_info(&self, _epoch: u64, _epoch_data: EpochData) -> Result<(), Error> {
        todo!("LRU epoch writes are not implemented in the skeleton")
    }
}

impl ObjectStore for LruMemoryStore {
    fn get_objects(&self, _keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        todo!("LRU object reads are not implemented in the skeleton")
    }
}

impl ObjectStoreWriter for LruMemoryStore {
    fn write_object(
        &self,
        _key: &ObjectKey,
        _object: Object,
        _actual_version: u64,
    ) -> Result<(), Error> {
        todo!("LRU object writes are not implemented in the skeleton")
    }
}

impl CheckpointStore for LruMemoryStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        todo!("LRU checkpoint reads are not implemented in the skeleton")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        todo!("LRU latest-checkpoint lookup is not implemented in the skeleton")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("LRU checkpoint-digest lookups are not implemented in the skeleton")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("LRU contents-digest lookups are not implemented in the skeleton")
    }
}

impl CheckpointStoreWriter for LruMemoryStore {
    fn write_checkpoint(&self, _checkpoint: &CheckpointData) -> Result<(), Error> {
        todo!("LRU checkpoint writes are not implemented in the skeleton")
    }
}

impl SetupStore for LruMemoryStore {
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        todo!("LRU setup is not implemented in the skeleton")
    }
}

impl StoreSummary for LruMemoryStore {
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(
            writer,
            "LruMemoryStore(node={}, txn_cap={}, epoch_cap={}, object_cap={}, root_map_cap={}, checkpoint_map_cap={})",
            self.node.network_name(),
            self.txn_cap,
            self.epoch_cap,
            self.object_cap,
            self.root_map_cap,
            self.checkpoint_map_cap
        )?;
        Ok(())
    }
}
