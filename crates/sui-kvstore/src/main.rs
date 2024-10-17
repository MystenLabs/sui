// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;
use async_trait::async_trait;
use sui_data_ingestion_core::{setup_single_workflow, Worker};
use sui_kvstore::{BigTableClient, KeyValueStore, TransactionData};
use sui_types::full_checkpoint_content::CheckpointData;

struct KvWorker {
    client: BigTableClient,
}

#[async_trait]
impl Worker for KvWorker {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
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
        client.save_checkpoint(checkpoint).await?;
        client.save_objects(&objects).await?;
        client.save_transactions(&transactions).await?;
        eprintln!(
            "finished processing checkpoint {}",
            checkpoint.checkpoint_summary.sequence_number
        );
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = BigTableClient::new("sui".to_string(), false, None).await?;
    let (executor, _term_sender) = setup_single_workflow(
        KvWorker { client },
        "https://checkpoints.mainnet.sui.io".to_string(),
        0,
        1,
        None,
    )
    .await?;
    executor.await?;
    Ok(())
}
