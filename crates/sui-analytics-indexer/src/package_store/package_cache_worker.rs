// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{package_store::PackageCache, Worker};

pub const PACKAGE_CACHE_WORKER_NAME: &str = "package_cache_manager";

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

    async fn process_checkpoint_arc(&self, checkpoint_data: &Arc<CheckpointData>) -> Result<()> {
        let sequence_number = *checkpoint_data.checkpoint_summary.sequence_number();
        let cache = self.package_cache.clone();
        let checkpoint_data = checkpoint_data.clone();

        tokio::task::spawn_blocking(move || {
            let all_objects = checkpoint_data
                .transactions
                .iter()
                .flat_map(|txn| txn.output_objects.iter());
            cache.update_batch(all_objects)?;
            Ok::<(), anyhow::Error>(())
        })
        .await??;

        self.package_cache.coordinator.mark_ready(sequence_number);
        Ok(())
    }

    fn preprocess_hook(&self, _: &CheckpointData) -> Result<()> {
        Ok(())
    }
}
