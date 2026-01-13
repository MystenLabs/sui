// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use bytes::Bytes;
use object_store::path::Path as ObjectPath;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::BatchStatus;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::full_checkpoint_content::{Checkpoint, CheckpointData};

pub struct BcsCheckpoint {
    pub sequence_number: u64,
    pub bcs_bytes: Bytes,
}

pub struct CheckpointBcsPipeline;

#[async_trait::async_trait]
impl Processor for CheckpointBcsPipeline {
    const NAME: &'static str = "checkpoint_bcs";
    type Value = BcsCheckpoint;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let sequence_number = checkpoint.summary.sequence_number;
        let checkpoint_data = CheckpointData::from(Checkpoint::clone(checkpoint));
        let bcs_bytes = Bytes::from(Blob::encode(&checkpoint_data, BlobEncoding::Bcs)?.to_bytes());

        Ok(vec![BcsCheckpoint {
            sequence_number,
            bcs_bytes,
        }])
    }
}

#[async_trait::async_trait]
impl Handler for CheckpointBcsPipeline {
    type Store = ObjectStore;
    type Batch = Option<Self::Value>;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
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
        let Some(blob) = batch else {
            return Ok(0);
        };

        let path = format!("{}.chk", blob.sequence_number);
        conn.object_store()
            .put(&ObjectPath::from(path), blob.bcs_bytes.clone().into())
            .await?;
        Ok(1)
    }
}
