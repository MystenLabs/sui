// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use futures::future::try_join_all;
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use tokio::sync::{Mutex, Semaphore};

use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::AnalyticsHandler;
use crate::tables::MoveCallEntry;
use crate::FileType;

pub struct MoveCallHandler {
    state: Mutex<State>,
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

        // Short-circuit if there is nothing to do.
        if checkpoint_transactions.is_empty() {
            return Ok(());
        }

        //---------------------------------------------------------------------
        // Concurrency scaffolding – semaphore chain
        //---------------------------------------------------------------------
        let n = checkpoint_transactions.len();
        let semaphores: Vec<Arc<Semaphore>> = (0..n).map(|_| Arc::new(Semaphore::new(0))).collect();
        // Allow the first task to flush immediately.
        semaphores[0].add_permits(1);

        // Take ownership of the transactions for the async tasks.
        let transactions: Vec<CheckpointTransaction> = checkpoint_transactions.clone();

        // Spawn a future per transaction.
        let mut futs = Vec::with_capacity(n);
        for (idx, tx) in transactions.into_iter().enumerate() {
            let sem_curr = semaphores[idx].clone();
            let sem_next = semaphores.get(idx + 1).cloned();

            let epoch = checkpoint_summary.epoch;
            let checkpoint = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;

            futs.push(async move {
                // -----------------------------------------------------------------
                // 1. Build the Move-call entries for *this* transaction.
                // -----------------------------------------------------------------
                let move_calls = tx
                    .transaction
                    .transaction_data()
                    .move_calls()
                    .iter()
                    .map(|(pkg, module, function)| MoveCallEntry {
                        transaction_digest: tx.transaction.digest().base58_encode(),
                        checkpoint,
                        epoch,
                        timestamp_ms,
                        package: pkg.to_string(),
                        module: module.to_string(),
                        function: function.to_string(),
                    })
                    .collect::<Vec<_>>();

                // -----------------------------------------------------------------
                // 2. Wait until it’s our turn to append to the shared state.
                // -----------------------------------------------------------------
                sem_curr.acquire().await.unwrap().forget();

                {
                    let mut state = self.state.lock().await;
                    state.move_calls.extend(move_calls);
                }

                // -----------------------------------------------------------------
                // 3. Signal the next transaction in the chain.
                // -----------------------------------------------------------------
                if let Some(next) = sem_next {
                    next.add_permits(1);
                }

                Ok::<(), anyhow::Error>(())
            });
        }

        // Execute all tasks and propagate the first error (if any).
        try_join_all(futs).await?;
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
        Self {
            state: Mutex::new(State {
                move_calls: Vec::new(),
            }),
        }
    }
}
