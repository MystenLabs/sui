// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::handlers::AnalyticsHandler;
use crate::tables::MovePackageEntry;
use crate::FileType;

#[derive(Clone)]
pub struct PackageHandler {}

impl PackageHandler {
    pub fn new() -> Self {
        PackageHandler {}
    }

    async fn process_transactions(
        &self,
        checkpoint_data: &CheckpointData,
    ) -> Result<Vec<MovePackageEntry>> {
        let txn_len = checkpoint_data.transactions.len();
        let mut packages = Vec::new();

        for idx in 0..txn_len {
            let transaction = &checkpoint_data.transactions[idx];
            let epoch = checkpoint_data.checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint_data.checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint_data.checkpoint_summary.timestamp_ms;

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
        }

        Ok(packages)
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<MovePackageEntry> for PackageHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: &CheckpointData,
    ) -> Result<Vec<MovePackageEntry>> {
        self.process_transactions(checkpoint_data).await
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::MovePackage)
    }

    fn name(&self) -> &'static str {
        "package"
    }
}
