// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use futures::future::try_join_all;
use std::sync::Arc;
use sui_data_ingestion_core::Worker;
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::object::Object;
use tokio::sync::{Mutex, Semaphore};

use crate::handlers::AnalyticsHandler;
use crate::tables::MovePackageEntry;
use crate::FileType;

pub struct PackageHandler {
    state: Mutex<State>,
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

        if checkpoint_transactions.is_empty() {
            return Ok(());
        }

        // --------------------------------------------------------------------
        // Semaphore‑chain: compute in parallel, flush in order.
        // --------------------------------------------------------------------
        let n = checkpoint_transactions.len();
        let sems: Vec<Arc<Semaphore>> = (0..n).map(|_| Arc::new(Semaphore::new(0))).collect();
        sems[0].add_permits(1);

        // Take ownership of txs for move into tasks.
        let txs: Vec<CheckpointTransaction> = checkpoint_transactions.clone();

        let mut futs = Vec::with_capacity(n);
        for (idx, tx) in txs.into_iter().enumerate() {
            let sem_cur = sems[idx].clone();
            let sem_next = sems.get(idx + 1).cloned();

            let epoch = checkpoint_summary.epoch;
            let checkpoint = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;

            let this = self;

            futs.push(async move {
                // 1. Local buffer
                let mut local = Vec::new();
                this.process_transaction(epoch, checkpoint, timestamp_ms, &tx, &mut local)?;

                // 2. Wait our turn
                sem_cur.acquire().await.unwrap().forget();
                {
                    let mut state = this.state.lock().await;
                    state.packages.extend(local);
                }

                // 3. Signal next
                if let Some(next) = sem_next {
                    next.add_permits(1);
                }

                Ok::<(), anyhow::Error>(())
            });
        }

        try_join_all(futs).await?;
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
        let state = Mutex::new(State {
            packages: Vec::new(),
        });
        Self { state }
    }

    fn process_transaction(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        out: &mut Vec<MovePackageEntry>,
    ) -> Result<()> {
        for object in checkpoint_transaction.output_objects.iter() {
            self.process_package(epoch, checkpoint, timestamp_ms, object, out)?;
        }
        Ok(())
    }

    fn process_package(
        &self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
        out: &mut Vec<MovePackageEntry>,
    ) -> Result<()> {
        if let sui_types::object::Data::Package(p) = &object.data {
            let entry = MovePackageEntry {
                package_id: p.id().to_string(),
                package_version: Some(p.version().value()),
                checkpoint,
                epoch,
                timestamp_ms,
                bcs: String::new(),
                bcs_length: bcs::to_bytes(object)?.len() as u64,
                transaction_digest: object.previous_transaction.to_string(),
                original_package_id: Some(p.original_package_id().to_string()),
            };
            out.push(entry);
        }
        Ok(())
    }
}
