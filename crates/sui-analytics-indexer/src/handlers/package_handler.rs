// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};
use sui_indexer::framework::Handler;
use sui_rest_api::CheckpointData;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::object::Object;

use crate::handlers::AnalyticsHandler;
use crate::tables::MovePackageEntry;
use crate::FileType;

pub struct PackageHandler {
    packages: Vec<MovePackageEntry>,
}

#[async_trait::async_trait]
impl Handler for PackageHandler {
    fn name(&self) -> &str {
        "package"
    }
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;
        for checkpoint_transaction in checkpoint_transactions {
            self.process_transaction(
                checkpoint_summary.epoch,
                checkpoint_summary.sequence_number,
                checkpoint_summary.timestamp_ms,
                checkpoint_transaction,
            )?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<MovePackageEntry> for PackageHandler {
    fn read(&mut self) -> Result<Vec<MovePackageEntry>> {
        let cloned = self.packages.clone();
        self.packages.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MovePackage)
    }
}

impl PackageHandler {
    pub fn new() -> Self {
        PackageHandler { packages: vec![] }
    }
    fn process_transaction(
        &mut self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
    ) -> Result<()> {
        for object in checkpoint_transaction.output_objects.iter() {
            self.process_package(epoch, checkpoint, timestamp_ms, object)?;
        }
        Ok(())
    }
    fn process_package(
        &mut self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        object: &Object,
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
                bcs: Base64::encode(bcs::to_bytes(p).unwrap()),
                transaction_digest: object.previous_transaction.to_string(),
                original_package_id: Some(original_package_id.to_string()),
            };
            self.packages.push(package)
        }
        Ok(())
    }
}
