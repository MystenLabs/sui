// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    models::display::StoredDisplay,
    types::{
        IndexedCheckpoint, IndexedDeletedObject, IndexedEpochInfo, IndexedEvent, IndexedObject,
        IndexedPackage, IndexedTransaction, TxIndex,
    },
};

pub mod checkpoint_handler;
pub mod committer;
pub mod objects_snapshot_processor;
pub mod tx_processor;

#[derive(Debug)]
pub struct CheckpointDataToCommit {
    pub checkpoint: IndexedCheckpoint,
    pub transactions: Vec<IndexedTransaction>,
    pub events: Vec<IndexedEvent>,
    pub tx_indices: Vec<TxIndex>,
    pub display_updates: BTreeMap<String, StoredDisplay>,
    pub object_changes: TransactionObjectChangesToCommit,
    pub object_history_changes: TransactionObjectChangesToCommit,
    pub packages: Vec<IndexedPackage>,
    pub epoch: Option<EpochToCommit>,
}

#[derive(Clone, Debug)]
pub struct TransactionObjectChangesToCommit {
    pub changed_objects: Vec<IndexedObject>,
    pub deleted_objects: Vec<IndexedDeletedObject>,
}

#[derive(Clone, Debug)]
pub struct EpochToCommit {
    pub last_epoch: Option<IndexedEpochInfo>,
    pub new_epoch: IndexedEpochInfo,
    pub network_total_transactions: u64,
}
