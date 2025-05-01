// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{package_store::PackageCache, Worker}; // your ingestion trait

pub const PACKAGE_CACHE_WORKER_NAME: &str = "package_cache_manager";

/// First stage of the analytics pipeline: make sure `PackageCache` contains
/// every object for the checkpoint and broadcast that fact.
/// Assumes it's concurrency is set to 1 so every checkpoint is processed serially.
pub struct PackageCacheWorker {
    package_cache: Arc<PackageCache>,
}

impl PackageCacheWorker {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self { package_cache }
    }

    pub fn name(&self) -> &'static str {
        PACKAGE_CACHE_WORKER_NAME
    }
}

#[async_trait]
impl Worker for PackageCacheWorker {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        let sequence_number = *checkpoint_data.checkpoint_summary.sequence_number();

        // Update / insert every output object from this checkpoint.
        for tx in &checkpoint_data.transactions {
            for object in &tx.output_objects {
                self.package_cache.update(object)?;
            }
        }

        self.package_cache.coordinator.mark_ready(sequence_number);

        Ok(())
    }

    fn preprocess_hook(&self, _: &CheckpointData) -> Result<()> {
        Ok(())
    }
}
