// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::models::addresses::Address;
use crate::models::checkpoints::Checkpoint;
use crate::models::events::Event;
use crate::models::objects::Object;
use crate::models::owners::OwnerChange;
use crate::models::packages::Package;
use crate::models::transactions::Transaction;
use async_trait::async_trait;
use sui_json_rpc_types::{
    Checkpoint as RpcCheckpoint, CheckpointId, SuiParsedObject, SuiTransactionResponse,
};

#[async_trait]
pub trait IndexerStore {
    fn get_latest_checkpoint_sequence_number(&self) -> Result<i64, IndexerError>;
    fn get_checkpoint(&self, id: CheckpointId) -> Result<Checkpoint, IndexerError>;

    fn get_total_transaction_number(&self) -> Result<i64, IndexerError>;

    // NOTE: PG table serial number does not always increment by 1
    // based on observations, thus `get_total_transaction_number` and
    // `get_latest_transaction_sequence_number` are not always equal.
    fn get_latest_transaction_sequence_number(&self) -> Result<i64, IndexerError>;

    // TODO: combine all get_transaction* methods
    fn get_transaction_by_digest(&self, txn_digest: String) -> Result<Transaction, IndexerError>;
    fn get_transaction_sequence_by_digest(
        &self,
        txn_digest: Option<String>,
        reverse: bool,
    ) -> Result<i64, IndexerError>;

    fn get_all_transaction_digest_page(
        &self,
        start_sequence: i64,
        limit: usize,
        reverse: bool,
    ) -> Result<Vec<String>, IndexerError>;

    fn get_transaction_digest_page_by_mutated_object(
        &self,
        object_id: String,
        start_sequence: i64,
        limit: usize,
        reverse: bool,
    ) -> Result<Vec<String>, IndexerError>;

    fn get_transaction_digest_page_by_sender_address(
        &self,
        sender_address: String,
        start_sequence: i64,
        limit: usize,
        reverse: bool,
    ) -> Result<Vec<String>, IndexerError>;

    fn get_transaction_digest_page_by_recipient_address(
        &self,
        recipient_address: String,
        start_sequence: i64,
        limit: usize,
        reverse: bool,
    ) -> Result<Vec<String>, IndexerError>;

    fn read_transactions(
        &self,
        last_processed_id: i64,
        limit: usize,
    ) -> Result<Vec<Transaction>, IndexerError>;

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
