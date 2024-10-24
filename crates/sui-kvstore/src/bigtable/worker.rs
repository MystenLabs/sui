// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{BigTableClient, KeyValueStoreWriter, TransactionData};
use async_trait::async_trait;
use sui_data_ingestion_core::Worker;
use sui_types::full_checkpoint_content::CheckpointData;

pub struct KvWorker {
    pub client: BigTableClient,
}

#[async_trait]
impl Worker for KvWorker {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> anyhow::Result<()> {
        let mut client = self.client.clone();
        let mut objects = vec![];
        let mut transactions = vec![];
        for transaction in &checkpoint.transactions {
            let full_transaction = TransactionData {
                transaction: transaction.transaction.clone(),
                effects: transaction.effects.clone(),
                events: transaction.events.clone(),
                checkpoint_number: checkpoint.checkpoint_summary.sequence_number,
                timestamp: checkpoint.checkpoint_summary.timestamp_ms,
            };
            for object in &transaction.output_objects {
                objects.push(object);
            }
            transactions.push(full_transaction);
        }
        client.save_objects(&objects).await?;
        client.save_transactions(&transactions).await?;
        client.save_checkpoint(checkpoint).await?;
        Ok(())
    }
}
