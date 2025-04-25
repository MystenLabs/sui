// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use sui_data_ingestion_core::Worker;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use tokio::sync::Mutex;

use crate::handlers::AnalyticsHandler;
use crate::tables::MovePackageEntry;
use crate::FileType;

pub struct PackageHandler {
    state: Mutex<BTreeMap<usize, Vec<MovePackageEntry>>>,
}

#[async_trait::async_trait]
impl Worker for PackageHandler {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        let checkpoint_summary = &checkpoint_data.checkpoint_summary;
        let checkpoint_transactions = &checkpoint_data.transactions;

        // Create a channel to collect results
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, Vec<MovePackageEntry>)>(
            checkpoint_transactions.len(),
        );

        // Process transactions in parallel
        let mut futures = Vec::new();

        for (idx, checkpoint_transaction) in checkpoint_transactions.iter().enumerate() {
            let tx = tx.clone();
            let transaction = checkpoint_transaction.clone();
            let epoch = checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_summary.timestamp_ms;

            // Spawn a task for each transaction
            let handle = tokio::spawn(async move {
                match Self::process_transaction(
                    epoch,
                    checkpoint_seq,
                    timestamp_ms,
                    &transaction,
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
        while let Some((idx, packages)) = rx.recv().await {
            state.insert(idx, packages);
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<MovePackageEntry> for PackageHandler {
    async fn read(&self) -> Result<Box<dyn Iterator<Item = MovePackageEntry>>> {
        let mut state = self.state.lock().await;
        let packages_map = std::mem::take(&mut *state);

        // Flatten the map into a single iterator in order by transaction index
        Ok(Box::new(packages_map.into_values().flatten()))
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
            state: Mutex::new(BTreeMap::new()),
        }
    }
    
    fn process_transaction(
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
    ) -> Result<Vec<MovePackageEntry>> {
        let mut packages = Vec::new();
        for object in checkpoint_transaction.output_objects.iter() {
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
                packages.push(package);
            }
        }
        Ok(packages)
    }
}
