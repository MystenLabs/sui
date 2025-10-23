// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{BigTableClient, KeyValueStoreReader, KeyValueStoreWriter, TransactionData};
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
        client
            .save_objects(&objects, checkpoint.checkpoint_summary.timestamp_ms)
            .await?;
        client.save_transactions(&transactions).await?;
        client.save_checkpoint(checkpoint).await?;
        if let Some(epoch_info) = checkpoint.epoch_info()? {
            if epoch_info.epoch > 0
                && let Some(mut prev) = client.get_epoch(epoch_info.epoch - 1).await?
            {
                prev.end_checkpoint = epoch_info.start_checkpoint.map(|sq| sq - 1);
                prev.end_timestamp_ms = epoch_info.start_timestamp_ms;
                client.save_epoch(prev).await?;
            }
            client.save_epoch(epoch_info).await?;
        }
        Ok(())
    }
}
