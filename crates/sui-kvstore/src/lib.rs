// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
mod bigtable;
use anyhow::Result;
use async_trait::async_trait;
pub use bigtable::client::BigTableClient;
use serde::{Deserialize, Serialize};
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSequenceNumber,
};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use sui_types::transaction::Transaction;

#[async_trait]
pub trait KeyValueStore {
    async fn get_objects(&mut self, objects: &[ObjectKey]) -> Result<Vec<Object>>;
    async fn get_transactions(
        &mut self,
        transactions: &[TransactionDigest],
    ) -> Result<Vec<TransactionData>>;
    async fn get_checkpoint(
        &mut self,
        sequence_number: SequenceNumber,
    ) -> Result<Option<Checkpoint>>;

    async fn save_objects(&mut self, objects: &[&Object]) -> Result<()>;
    async fn save_transactions(&mut self, transactions: &[TransactionData]) -> Result<()>;
    async fn save_checkpoint(&mut self, checkpoint: &CheckpointData) -> Result<()>;
}

pub type Checkpoint = (CertifiedCheckpointSummary, CheckpointContents);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionData {
    pub transaction: Transaction,
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub checkpoint_number: CheckpointSequenceNumber,
    pub timestamp: u64,
}
