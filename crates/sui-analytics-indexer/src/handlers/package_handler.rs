// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use futures::{stream, StreamExt};
use sui_data_ingestion_core::Worker;
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::sync::Mutex;

use crate::handlers::AnalyticsHandler;
use crate::tables::MovePackageEntry;
use crate::FileType;

#[derive(Clone)]
pub struct PackageHandler {
    state: Arc<Mutex<Vec<MovePackageEntry>>>,
}

#[async_trait::async_trait]
impl Worker for PackageHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        // Process transactions in parallel using buffered stream for ordered execution
        let txn_len = checkpoint_data.transactions.len();
        let mut entries = Vec::new();
        
        let mut stream = stream::iter(0..txn_len)
            .map(|idx| {
                let cp = checkpoint_data.clone();
                tokio::spawn(async move { 
                    handle_tx(idx, &cp).await
                })
            })
            .buffered(num_cpus::get() * 4);

        while let Some(join_res) = stream.next().await {
            match join_res {
                Ok(Ok(tx_entries)) => {
                    entries.extend(tx_entries);
                }
                Ok(Err(e)) => {
                    // Task executed but application logic returned an error
                    return Err(e);
                }
                Err(e) => {
                    // Task panicked or was cancelled
                    return Err(anyhow::anyhow!("Task join error: {}", e));
                }
            }
        }

        // Store results
        *self.state.lock().await = entries;
        
        Ok(())
    }
}

/// Private per-tx helper for processing individual transactions
async fn handle_tx(
    tx_idx: usize, 
    checkpoint: &CheckpointData
) -> Result<Vec<MovePackageEntry>> {
    let transaction = &checkpoint.transactions[tx_idx];
    let epoch = checkpoint.checkpoint_summary.epoch;
    let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number;
    let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

    let mut packages = Vec::new();
    for object in transaction.output_objects.iter() {
        if let sui_types::object::Data::Package(p) = &object.data {
            let package_id = p.id();
            let package_version = p.version().value();
            let original_package_id = p.original_package_id();
            let package = MovePackageEntry {
                package_id: package_id.to_string(),
                package_version: Some(package_version),
                checkpoint: checkpoint_seq,
                epoch,
                timestamp_ms,
                bcs: "".to_string(),
                bcs_length: bcs::to_bytes(object).unwrap().len() as u64,
                transaction_digest: object.previous_transaction.to_string(),
                original_package_id: Some(original_package_id.to_string()),
            };
            packages.push(package);
        }
    }
    Ok(packages)
}

#[async_trait::async_trait]
impl AnalyticsHandler<MovePackageEntry> for PackageHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = MovePackageEntry>>> {
        let mut state = self.state.lock().await;
        let entries = std::mem::take(&mut *state);

        // Return all entries
        Ok(Box::new(entries.into_iter()))
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MovePackage)
    }

    fn name(&self) -> &'static str {
        "package"
    }
}

impl PackageHandler {
    pub fn new() -> Self {
        PackageHandler {
            state: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
