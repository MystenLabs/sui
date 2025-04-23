// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::{AnalyticsHandler, InputObjectTracker, ObjectStatusTracker};
use crate::tables::TransactionObjectEntry;
use crate::FileType;

#[derive(Clone)]
pub struct TransactionObjectsHandler {
    state: Arc<Mutex<State>>,
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

        // ──────────────────────────────────────────────────────────────────────────
        // Build a Semaphore chain so we can push results to `state.transaction_objects` in the
        // same order as `checkpoint_transactions`, while allowing *everything*
        // else to run in parallel.
        // ──────────────────────────────────────────────────────────────────────────
        let txn_count = checkpoint_transactions.len();
        let semaphores: Vec<_> = (0..txn_count)
            .map(|i| {
                if i == 0 {
                    Arc::new(Semaphore::new(1)) // first txn proceeds immediately
                } else {
                    Arc::new(Semaphore::new(0))
                }
            })
            .collect();

        let mut handles: Vec<JoinHandle<Result<()>>> = Vec::with_capacity(txn_count);

        for (idx, checkpoint_transaction) in checkpoint_transactions.iter().cloned().enumerate() {
            let handler = self.clone();
            let start_sem = semaphores[idx].clone();
            let next_sem = semaphores.get(idx + 1).cloned();

            // Snapshot any data we need from the summary (Copy types, cheap).
            let epoch = checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;

            let handle = tokio::spawn(async move {
                // ───── 1. Heavy work off‑mutex ───────────────────────────────────
                let mut local_state = State {
                    transaction_objects: Vec::new(),
                };

                handler.process_transaction(
                    epoch,
                    checkpoint_seq,
                    timestamp_ms,
                    &checkpoint_transaction,
                    &checkpoint_transaction.effects,
                    &mut local_state,
                );

                // ───── 2. Append results in order ────────────────────────────────
                // Wait for our turn.
                let _permit = start_sem.acquire().await?;

                {
                    let mut shared_state = handler.state.lock().await;
                    shared_state
                        .transaction_objects
                        .extend(local_state.transaction_objects.into_iter());
                }

                // Signal the next task.
                if let Some(next) = next_sem {
                    next.add_permits(1);
                }

                Ok(())
            });

            handles.push(handle);
        }

        // Propagate any error.
        for h in handles {
            h.await??;
        }

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
            state: Arc::new(Mutex::new(State {
                transaction_objects: vec![],
            })),
        }
    }
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
        // input
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
        // output
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
    // Transaction object data.
    // Builds a view of the object in input and output of a transaction.
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
