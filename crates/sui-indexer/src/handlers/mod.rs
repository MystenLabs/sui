// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod checkpoint_handler;
pub mod checkpoint_handler_v2;
pub mod committer;
pub mod tx_processor;

use std::collections::BTreeMap;

use sui_types::base_types::ObjectRef;

use crate::{
    models_v2::display::StoredDisplay,
    types_v2::{
        IndexedCheckpoint, IndexedEpochInfo, IndexedEvent, IndexedObject, IndexedPackage,
        IndexedTransaction, TxIndex,
    },
};

#[derive(Debug)]
pub struct CheckpointDataToCommit {
    pub checkpoint: IndexedCheckpoint,
    pub transactions: Vec<IndexedTransaction>,
    pub events: Vec<IndexedEvent>,
    pub tx_indices: Vec<TxIndex>,
    pub display_updates: BTreeMap<String, StoredDisplay>,
    pub object_changes: TransactionObjectChangesToCommit,
    pub packages: Vec<IndexedPackage>,
    pub epoch: Option<EpochToCommit>,
}

#[derive(Debug)]
pub struct TransactionObjectChangesToCommit {
    pub changed_objects: Vec<IndexedObject>,
    pub deleted_objects: Vec<ObjectRef>,
}

#[derive(Debug)]
pub struct EpochToCommit {
    pub last_epoch: Option<IndexedEpochInfo>,
    pub new_epoch: IndexedEpochInfo,
}
