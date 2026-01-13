// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework_store_traits::Store;
use sui_types::full_checkpoint_content::{Checkpoint, CheckpointData};

use crate::bigtable::store::BigTableStore;

/// Pipeline that writes checkpoint data to BigTable.
/// Mirrors the behavior of the legacy KvWorker.
pub struct KvStorePipeline;

#[async_trait::async_trait]
impl Processor for KvStorePipeline {
    const NAME: &'static str = "kvstore";
    type Value = CheckpointData;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        // Convert from new Checkpoint type to legacy CheckpointData
        Ok(vec![checkpoint.as_ref().clone().into()])
    }
}

#[async_trait::async_trait]
impl Handler for KvStorePipeline {
    type Store = BigTableStore;
    type Batch = Option<CheckpointData>;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        // Take one checkpoint at a time. Combined with write_concurrency,
        // this allows multiple checkpoints in flight while ensuring retries
        // only affect a single checkpoint.
        if batch.is_none() && values.len() > 0 {
            *batch = values.next();
            BatchStatus::Ready
        } else {
            BatchStatus::Pending
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        if let Some(checkpoint) = batch {
            conn.process_checkpoint(checkpoint).await?;
            Ok(1)
        } else {
            Ok(0)
        }
    }
}
