// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};

use sui_types::full_checkpoint_content::CheckpointData;

use crate::FileType;
use crate::handlers::{AnalyticsHandler, TransactionProcessor, process_transactions};
use crate::tables::PackageBCSEntry;

#[derive(Clone)]
pub struct PackageBCSHandler {}

impl PackageBCSHandler {
    pub fn new() -> Self {
        PackageBCSHandler {}
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<PackageBCSEntry> for PackageBCSHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: &Arc<CheckpointData>,
    ) -> Result<Box<dyn Iterator<Item = PackageBCSEntry> + Send + Sync>> {
        process_transactions(checkpoint_data.clone(), Arc::new(self.clone())).await
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MovePackageBCS)
    }

    fn name(&self) -> &'static str {
        "package_bcs"
    }
}

#[async_trait::async_trait]
impl TransactionProcessor<PackageBCSEntry> for PackageBCSHandler {
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Box<dyn Iterator<Item = PackageBCSEntry> + Send + Sync>> {
        let transaction = &checkpoint.transactions[tx_idx];
        let epoch = checkpoint.checkpoint_summary.epoch;
        let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number;
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

        let mut packages = Vec::new();
        for object in transaction.output_objects.iter() {
            if let sui_types::object::Data::Package(_p) = &object.data {
                let package_id = object.id();
                let entry = PackageBCSEntry {
                    package_id: package_id.to_string(),
                    checkpoint: checkpoint_seq,
                    epoch,
                    timestamp_ms,
                    bcs: Base64::encode(bcs::to_bytes(object).unwrap()),
                };
                packages.push(entry);
            }
        }

        Ok(Box::new(packages.into_iter()))
    }
}
