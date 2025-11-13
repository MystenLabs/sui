// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::encoding::{Base64, Encoding};
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::EpochId;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::tables::PackageBCSEntry;
use crate::{AnalyticsBatch, AnalyticsHandler, AnalyticsMetadata, FileType};

pub struct PackageBCSProcessor;

pub type PackageBCSHandler = AnalyticsHandler<PackageBCSProcessor, AnalyticsBatch<PackageBCSEntry>>;

impl AnalyticsMetadata for PackageBCSEntry {
    const FILE_TYPE: FileType = FileType::MovePackageBCS;

    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint_sequence_number(&self) -> u64 {
        self.checkpoint
    }
}

#[async_trait]
impl Processor for PackageBCSProcessor {
    const NAME: &'static str = "move_package_bcs";
    const FANOUT: usize = 10;
    type Value = PackageBCSEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut packages = Vec::new();
        for transaction in &checkpoint.transactions {
            for object in transaction.output_objects(&checkpoint.object_set) {
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
        }

        Ok(packages)
    }
}
