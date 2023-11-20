// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};

use sui_indexer::framework::Handler;
use sui_rest_api::CheckpointData;
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
            checkpoint_transaction
                .output_objects
                .iter()
                .for_each(|object| {
                    self.process_package(
                        checkpoint_summary.epoch,
                        checkpoint_summary.sequence_number,
                        checkpoint_summary.timestamp_ms,
                        object,
                    )
                });
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
    fn process_package(&mut self, epoch: u64, checkpoint: u64, timestamp_ms: u64, object: &Object) {
        if let sui_types::object::Data::Package(p) = &object.data {
            let package = MovePackageEntry {
                package_id: p.id().to_string(),
                checkpoint,
                epoch,
                timestamp_ms,
                bcs: Base64::encode(bcs::to_bytes(p).unwrap()),
                transaction_digest: object.previous_transaction.to_string(),
            };
            self.packages.push(package)
        }
    }
}
