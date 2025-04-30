// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use sui_types::{full_checkpoint_content::CheckpointData, SYSTEM_PACKAGE_ADDRESSES};

use crate::{
    package_store::PackageCache,
    Worker, // <-- the ingestion trait
};

const NAME: &str = "package_cache";

/// The first stage of the analytics pipeline: make sure `PackageCache`
/// contains every object for the checkpoint, broadcast that fact, and
/// evict system packages at an epoch boundary.
pub struct PackageCacheWorker {
    package_cache: Arc<PackageCache>,
}

impl PackageCacheWorker {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self { package_cache }
    }

    pub fn name(&self) -> &'static str {
        NAME
    }
}

#[async_trait]
impl Worker for PackageCacheWorker {
    /// No rows to emit – we just return `()`.
    type Result = ();

    async fn process_checkpoint(&self, ckpt: Arc<CheckpointData>) -> Result<()> {
        let epoch = ckpt.checkpoint_summary.epoch();
        let number = *ckpt.checkpoint_summary.sequence_number();

        // 1️⃣  Update / insert every output object from this checkpoint.
        for tx in &ckpt.transactions {
            for obj in &tx.output_objects {
                self.package_cache.update(obj)?;
            }
        }

        // 2️⃣  On epoch boundary evict system packages (newer data is always safe).
        if ckpt.checkpoint_summary.end_of_epoch_data.is_some() {
            self.package_cache
                .resolver
                .package_store()
                .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
        }

        // 3️⃣  Tell downstream handlers they can safely process (epoch, checkpoint).
        self.package_cache.coordinator.mark_ready(epoch, number);

        Ok(())
    }

    // No preprocessing needed for this worker.
    fn preprocess_hook(&self, _: &CheckpointData) -> Result<()> {
        Ok(())
    }
}
