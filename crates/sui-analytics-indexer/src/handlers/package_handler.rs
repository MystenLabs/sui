// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::object::Object;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

use crate::handlers::AnalyticsHandler;
use crate::tables::MovePackageEntry;
use crate::FileType;

#[derive(Clone)]
pub struct PackageHandler {
    state: Arc<Mutex<State>>,
}

struct State {
    packages: Vec<MovePackageEntry>,
}

#[async_trait::async_trait]
impl Worker for PackageHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;

        // ──────────────────────────────────────────────────────────────────────────
        // Build a Semaphore chain so we can push results to `state.packages` in the
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
                    packages: Vec::new(),
                };

                handler.process_transaction(
                    epoch,
                    checkpoint_seq,
                    timestamp_ms,
                    &checkpoint_transaction,
                    &mut local_state,
                )?;

                // ───── 2. Append results in order ────────────────────────────────
                // Wait for our turn.
                let _permit = start_sem.acquire().await?;

                {
                    let mut shared_state = handler.state.lock().await;
                    shared_state
                        .packages
                        .extend(local_state.packages.into_iter());
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
impl AnalyticsHandler<MovePackageEntry> for PackageHandler {
    async fn read(&self) -> Result<Vec<MovePackageEntry>> {
        let mut state = self.state.lock().await;
        Ok(std::mem::take(&mut state.packages))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MovePackage)
    }

    fn name(&self) -> &str {
        "package"
    }
}

impl PackageHandler {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(State { packages: vec![] }));
        PackageHandler { state }
    }
    fn process_transaction(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        state: &mut State,
    ) -> Result<()> {
        for object in checkpoint_transaction.output_objects.iter() {
            self.process_package(epoch, checkpoint, timestamp_ms, object, state)?;
        }
        Ok(())
    }
    fn process_package(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        state: &mut State,
    ) -> Result<()> {
        if let sui_types::object::Data::Package(p) = &object.data {
            let package_id = p.id();
            let package_version = p.version().value();
            let original_package_id = p.original_package_id();
            let package = MovePackageEntry {
                package_id: package_id.to_string(),
                package_version: Some(package_version),
                checkpoint,
                epoch,
                timestamp_ms,
                bcs: "".to_string(),
                bcs_length: bcs::to_bytes(object).unwrap().len() as u64,
                transaction_digest: object.previous_transaction.to_string(),
                original_package_id: Some(original_package_id.to_string()),
            };
            state.packages.push(package)
        }
        Ok(())
    }
}
