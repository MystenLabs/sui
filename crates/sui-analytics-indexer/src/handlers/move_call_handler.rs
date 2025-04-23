// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::AnalyticsHandler;
use crate::tables::MoveCallEntry;
use crate::FileType;

#[derive(Clone)]
pub struct MoveCallHandler {
    state: Arc<Mutex<State>>,
}

struct State {
    move_calls: Vec<MoveCallEntry>,
}

#[async_trait::async_trait]
impl Worker for MoveCallHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;

        // ──────────────────────────────────────────────────────────────────────────
        // Build a Semaphore chain so we can push results to `state.move_calls` in the
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
            let transaction_digest = checkpoint_transaction.transaction.digest().base58_encode();

            let move_calls: Vec<(String, String, String)> = checkpoint_transaction
                .transaction
                .transaction_data()
                .move_calls()
                .iter()
                .map(|(package, module, function)| {
                    (
                        package.to_string(),
                        module.to_string(),
                        function.to_string(),
                    )
                })
                .collect();

            let handle = tokio::spawn(async move {
                // ───── 1. Heavy work off‑mutex ───────────────────────────────────
                let mut local_state = State {
                    move_calls: Vec::new(),
                };

                handler.process_move_calls(
                    epoch,
                    checkpoint_seq,
                    timestamp_ms,
                    transaction_digest,
                    &move_calls,
                    &mut local_state,
                );

                // ───── 2. Append results in order ────────────────────────────────
                // Wait for our turn.
                let _permit = start_sem.acquire().await?;

                {
                    let mut shared_state = handler.state.lock().await;
                    shared_state
                        .move_calls
                        .extend(local_state.move_calls.into_iter());
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
impl AnalyticsHandler<MoveCallEntry> for MoveCallHandler {
    async fn read(&self) -> Result<Vec<MoveCallEntry>> {
        let mut state = self.state.lock().await;
        Ok(std::mem::take(&mut state.move_calls))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MoveCall)
    }

    fn name(&self) -> &str {
        "move_call"
    }
}

impl MoveCallHandler {
    pub fn new() -> Self {
        let state = State { move_calls: vec![] };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }
    // Process move calls with owned strings
    fn process_move_calls(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        transaction_digest: String,
        move_calls: &[(String, String, String)],
        state: &mut State,
    ) {
        for (package, module, function) in move_calls.iter() {
            let entry = MoveCallEntry {
                transaction_digest: transaction_digest.clone(),
                checkpoint,
                epoch,
                timestamp_ms,
                package: package.clone(),
                module: module.clone(),
                function: function.clone(),
            };
            state.move_calls.push(entry);
        }
    }
}
