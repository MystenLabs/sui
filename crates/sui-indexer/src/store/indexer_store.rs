// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::models::addresses::Address;
use crate::models::checkpoints::Checkpoint;
use crate::models::events::Event;
use crate::models::objects::Object;
use crate::models::owners::OwnerChange;
use crate::models::transactions::Transaction;
use async_trait::async_trait;
use sui_json_rpc_types::{Checkpoint as RpcCheckpoint, SuiParsedObject, SuiTransactionResponse};
use crate::models::packages::Package;

#[async_trait]
pub trait IndexerStore {
    fn get_latest_checkpoint_sequence_number(&self) -> Result<i64, IndexerError>;
    fn get_checkpoint(&self, checkpoint_sequence_number: i64) -> Result<Checkpoint, IndexerError>;

    fn persist_checkpoint(&self, data: &TemporaryCheckpointStore) -> Result<usize, IndexerError>;
    fn persist_epoch(&self, data: &TemporaryEpochStore) -> Result<usize, IndexerError>;

    fn log_errors(&self, errors: Vec<IndexerError>) -> Result<(), IndexerError>;
}

pub struct CheckpointData {
    pub checkpoint: RpcCheckpoint,
    pub transactions: Vec<SuiTransactionResponse>,
    pub objects: Vec<SuiParsedObject>,
}

// Per checkpoint indexing
pub struct TemporaryCheckpointStore {
    pub checkpoint: Checkpoint,
    pub transactions: Vec<Transaction>,
    pub events: Vec<Event>,
    pub objects: Vec<Object>,
    pub owner_changes: Vec<OwnerChange>,
    pub addresses: Vec<Address>,
    pub packages: Vec<Package>,
}

// Per epoch indexing
pub struct TemporaryEpochStore {
    pub owner_index: Vec<OwnerChange>,
}
