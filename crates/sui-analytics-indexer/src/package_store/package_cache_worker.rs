use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{stream, StreamExt};
use std::sync::Arc;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{package_store::PackageCache, Worker, ASYNC_TRANSACTIONS_TO_BUFFER};

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

    async fn process_checkpoint_arc(&self, checkpoint_data: Arc<CheckpointData>) -> Result<()> {
        let sequence_number = *checkpoint_data.checkpoint_summary.sequence_number();

        let txn_len = checkpoint_data.transactions.len();
        let cache = self.package_cache.clone();
        let mut stream = stream::iter(0..txn_len)
            .map(move |idx| {
                // move clones into the task
                let checkpoint_data = checkpoint_data.clone();
                let cache = cache.clone();

                tokio::spawn(async move {
                    let transaction = &checkpoint_data.transactions[idx];
                    for object in &transaction.output_objects {
                        cache.update(object)?;
                    }
                    Ok(())
                })
            })
            .buffered(*ASYNC_TRANSACTIONS_TO_BUFFER);

        while let Some(join_res) = stream.next().await {
            match join_res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(anyhow!(format!("task join error: {e}"))),
            }
        }

        self.package_cache.coordinator.mark_ready(sequence_number);
        Ok(())
    }

    fn preprocess_hook(&self, _: &CheckpointData) -> Result<()> {
        Ok(())
    }
}
