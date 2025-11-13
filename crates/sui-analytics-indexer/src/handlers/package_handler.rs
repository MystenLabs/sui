// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::EpochId;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::tables::MovePackageEntry;
use crate::{AnalyticsBatch, AnalyticsHandler, AnalyticsMetadata, FileType};

pub struct PackageProcessor;

pub type PackageHandler = AnalyticsHandler<PackageProcessor, AnalyticsBatch<MovePackageEntry>>;

impl AnalyticsMetadata for MovePackageEntry {
    const FILE_TYPE: FileType = FileType::MovePackage;

    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint_sequence_number(&self) -> u64 {
        self.checkpoint
    }
}

#[async_trait]
impl Processor for PackageProcessor {
    const NAME: &'static str = "move_package";
    const FANOUT: usize = 10;
    type Value = MovePackageEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut packages = Vec::new();

        for checkpoint_transaction in &checkpoint.transactions {
            for object in checkpoint_transaction.output_objects(&checkpoint.object_set) {
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
