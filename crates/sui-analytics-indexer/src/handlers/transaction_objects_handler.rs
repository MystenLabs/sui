// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use futures::future::try_join_all;
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use tokio::sync::{Mutex, Semaphore};

use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::{AnalyticsHandler, InputObjectTracker, ObjectStatusTracker};
use crate::tables::TransactionObjectEntry;
use crate::FileType;

pub struct TransactionObjectsHandler {
    state: Mutex<State>,
}

struct State {
    transaction_objects: Vec<TransactionObjectEntry>,
}

#[async_trait::async_trait]
impl Worker for TransactionObjectsHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;

        // Early-out if the checkpoint is empty.
        if checkpoint_transactions.is_empty() {
            return Ok(());
        }

        // ──────────────────────────────────────────────────────────────────────
        // Concurrency plumbing – one semaphore per transaction.
        // ──────────────────────────────────────────────────────────────────────
        let n = checkpoint_transactions.len();
        let semaphores: Vec<Arc<Semaphore>> = (0..n).map(|_| Arc::new(Semaphore::new(0))).collect();
        // Let the first tx flush immediately.
        semaphores[0].add_permits(1);

        // Take ownership of the vec so we can move its items into tasks.
        let transactions: Vec<CheckpointTransaction> = checkpoint_transactions.clone();

        let mut futs = Vec::with_capacity(n);
        for (idx, tx) in transactions.into_iter().enumerate() {
            let sem_curr = semaphores[idx].clone();
            let sem_next = semaphores.get(idx + 1).cloned();
            let epoch = checkpoint_summary.epoch;
            let checkpoint = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;
            let this = self;

            futs.push(async move {
                // 1. Build a local buffer for this transaction.
                let mut local_state = State {
                    transaction_objects: Vec::new(),
                };

                this.process_transaction(
                    epoch,
                    checkpoint,
                    timestamp_ms,
                    &tx,
                    &tx.effects,
                    &mut local_state,
                );

                // 2. Wait for our turn to append.
                sem_curr.acquire().await.unwrap().forget();

                {
                    let mut state = this.state.lock().await;
                    state
                        .transaction_objects
                        .extend(local_state.transaction_objects);
                }

                // 3. Unblock the next task in the chain.
                if let Some(next) = sem_next {
                    next.add_permits(1);
                }

                Ok::<(), anyhow::Error>(())
            });
        }

        // Drive all tasks and surface any error.
        try_join_all(futs).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionObjectEntry> for TransactionObjectsHandler {
    async fn read(&self) -> Result<Vec<TransactionObjectEntry>> {
        let mut state = self.state.lock().await;
        Ok(std::mem::take(&mut state.transaction_objects))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::TransactionObjects)
    }

    fn name(&self) -> &str {
        "transaction_objects"
    }
}

impl TransactionObjectsHandler {
    pub fn new() -> Self {
        TransactionObjectsHandler {
            state: Mutex::new(State {
                transaction_objects: vec![],
            }),
        }
    }

    /// Collects all `TransactionObjectEntry`s for a single checkpoint-transaction
    /// into `state`.  (Called from inside each parallel task.)
    fn process_transaction(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        effects: &TransactionEffects,
        state: &mut State,
    ) {
        let transaction = &checkpoint_transaction.transaction;
        let transaction_digest = transaction.digest().base58_encode();
        let txn_data = transaction.transaction_data();
        let input_object_tracker = InputObjectTracker::new(txn_data);
        let object_status_tracker = ObjectStatusTracker::new(effects);

        // Input objects
        txn_data
            .input_objects()
            .expect("Input objects must be valid")
            .iter()
            .map(|object| (object.object_id(), object.version().map(|v| v.value())))
            .for_each(|(object_id, version)| {
                self.process_transaction_object(
                    epoch,
                    checkpoint,
                    timestamp_ms,
                    transaction_digest.clone(),
                    &object_id,
                    version,
                    &input_object_tracker,
                    &object_status_tracker,
                    state,
                )
            });

        // Output objects
        checkpoint_transaction
            .output_objects
            .iter()
            .map(|object| (object.id(), Some(object.version().value())))
            .for_each(|(object_id, version)| {
                self.process_transaction_object(
                    epoch,
                    checkpoint,
                    timestamp_ms,
                    transaction_digest.clone(),
                    &object_id,
                    version,
                    &input_object_tracker,
                    &object_status_tracker,
                    state,
                )
            });
    }

    /// Records one `(object_id, version)` row for either the input- or output-side
    /// view of a transaction.
    fn process_transaction_object(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        transaction_digest: String,
        object_id: &ObjectID,
        version: Option<u64>,
        input_object_tracker: &InputObjectTracker,
        object_status_tracker: &ObjectStatusTracker,
        state: &mut State,
    ) {
        let entry = TransactionObjectEntry {
            object_id: object_id.to_string(),
            version,
            transaction_digest,
            checkpoint,
            epoch,
            timestamp_ms,
            input_kind: input_object_tracker.get_input_object_kind(object_id),
            object_status: object_status_tracker.get_object_status(object_id),
        };
        state.transaction_objects.push(entry);
    }
}
