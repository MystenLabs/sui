// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::encoding::{Base64, Encoding};
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::EpochId;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::parquet::ParquetBatch;
use crate::tables::PackageBCSEntry;
use crate::{AnalyticsBatch, AnalyticsHandler, CheckpointMetadata, FileType, PipelineConfig};

pub struct PackageBCSBatch {
    pub inner: ParquetBatch<PackageBCSEntry>,
}

impl Default for PackageBCSBatch {
    fn default() -> Self {
        Self {
            inner: ParquetBatch::new(FileType::MovePackageBCS, 0)
                .expect("Failed to create ParquetBatch"),
        }
    }
}

impl CheckpointMetadata for PackageBCSEntry {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint_sequence_number(&self) -> u64 {
        self.checkpoint
    }
}

impl AnalyticsBatch for PackageBCSBatch {
    type Entry = PackageBCSEntry;

    fn inner_mut(&mut self) -> &mut ParquetBatch<Self::Entry> {
        &mut self.inner
    }

    fn inner(&self) -> &ParquetBatch<Self::Entry> {
        &self.inner
    }
}

pub struct PackageBCSProcessor;

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

pub type PackageBCSHandler = AnalyticsHandler<PackageBCSProcessor, PackageBCSBatch>;
