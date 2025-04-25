// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use sui_data_ingestion_core::Worker;
use tokio::sync::Mutex;

use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::{AnalyticsHandler, InputObjectTracker, ObjectStatusTracker};
use crate::tables::TransactionObjectEntry;
use crate::FileType;

pub struct TransactionObjectsHandler {
    state: Mutex<BTreeMap<usize, Vec<TransactionObjectEntry>>>,
}

#[async_trait::async_trait]
impl Worker for TransactionObjectsHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        let checkpoint_summary = &checkpoint_data.checkpoint_summary;
        let checkpoint_transactions = &checkpoint_data.transactions;

        // Create a channel to collect results
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, Vec<TransactionObjectEntry>)>(
            checkpoint_transactions.len(),
        );

        // Process transactions in parallel
        let mut futures = Vec::new();

        for (idx, _checkpoint_transaction) in checkpoint_transactions.iter().enumerate() {
            let tx = tx.clone();
            let epoch = checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;
            let checkpoint_data_clone = checkpoint_data.clone();

            // Spawn a task for each transaction
            let handle = tokio::spawn(async move {
                let transaction = &checkpoint_data_clone.transactions[idx];
                match Self::process_transaction(
                    epoch,
                    checkpoint_seq,
                    timestamp_ms,
                    transaction,
                    &transaction.effects,
                ) {
                    Ok(entries) => {
                        if !entries.is_empty() {
                            let _ = tx.send((idx, entries)).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error processing transaction at index {}: {}", idx, e);
                    }
                }
            });

            futures.push(handle);
        }

        // Drop the original sender so the channel can close when all tasks are done
        drop(tx);

        // Wait for all tasks to complete
        for handle in futures {
            if let Err(e) = handle.await {
                tracing::error!("Task panicked: {}", e);
            }
        }

        // Collect results into the state in order by transaction index
        let mut state = self.state.lock().await;
        while let Some((idx, transaction_objects)) = rx.recv().await {
            state.insert(idx, transaction_objects);
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionObjectEntry> for TransactionObjectsHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = TransactionObjectEntry>>> {
        let mut state = self.state.lock().await;
        let transactions_map = std::mem::take(&mut *state);

        // Flatten the map into a single iterator in order by transaction index
        Ok(Box::new(transactions_map.into_values().flatten()))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::TransactionObjects)
    }

    fn name(&self) -> &'static str {
        "transaction_objects"
    }
}

impl TransactionObjectsHandler {
    pub fn new() -> Self {
        TransactionObjectsHandler {
            state: Mutex::new(BTreeMap::new()),
        }
    }
    
    fn process_transaction(
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        effects: &TransactionEffects,
    ) -> Result<Vec<TransactionObjectEntry>> {
        let transaction = &checkpoint_transaction.transaction;
        let transaction_digest = transaction.digest().base58_encode();
        let txn_data = transaction.transaction_data();
        let input_object_tracker = InputObjectTracker::new(txn_data);
        let object_status_tracker = ObjectStatusTracker::new(effects);
        let mut transaction_objects = Vec::new();
        
        // input
        for object in txn_data.input_objects().expect("Input objects must be valid").iter() {
            let object_id = object.object_id();
            let version = object.version().map(|v| v.value());
            let entry = Self::create_transaction_object_entry(
                epoch,
                checkpoint,
                timestamp_ms,
                transaction_digest.clone(),
                &object_id,
                version,
                &input_object_tracker,
                &object_status_tracker,
            );
            transaction_objects.push(entry);
        }
        
        // output
        for object in checkpoint_transaction.output_objects.iter() {
            let object_id = object.id();
            let version = Some(object.version().value());
            let entry = Self::create_transaction_object_entry(
                epoch,
                checkpoint,
                timestamp_ms,
                transaction_digest.clone(),
                &object_id,
                version,
                &input_object_tracker,
                &object_status_tracker,
            );
            transaction_objects.push(entry);
        }
        
        Ok(transaction_objects)
    }
    
    // Transaction object data.
    // Builds a view of the object in input and output of a transaction.
    fn create_transaction_object_entry(
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        transaction_digest: String,
        object_id: &ObjectID,
        version: Option<u64>,
        input_object_tracker: &InputObjectTracker,
        object_status_tracker: &ObjectStatusTracker,
    ) -> TransactionObjectEntry {
        TransactionObjectEntry {
            object_id: object_id.to_string(),
            version,
            transaction_digest,
            checkpoint,
            epoch,
            timestamp_ms,
            input_kind: input_object_tracker.get_input_object_kind(object_id),
            object_status: object_status_tracker.get_object_status(object_id),
        }
    }
}
