// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use object_store::path::Path as ObjectPath;
use object_store::{Error as ObjectStoreError, PutMode, PutPayload};
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::pipeline::{Processor, concurrent::BatchStatus};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_types::full_checkpoint_content::Checkpoint;

pub struct EpochsPipeline;

pub struct EpochCheckpoint {
    pub checkpoint_number: u64,
}

#[async_trait::async_trait]
impl Processor for EpochsPipeline {
    const NAME: &'static str = "epochs";
    type Value = EpochCheckpoint;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        if checkpoint.summary.is_last_checkpoint_of_epoch() {
            Ok(vec![EpochCheckpoint {
                checkpoint_number: checkpoint.summary.sequence_number,
            }])
        } else {
            Ok(vec![])
        }
    }
}

#[async_trait::async_trait]
impl Handler for EpochsPipeline {
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
        let Some(epoch_checkpoint) = batch else {
            return Ok(0);
        };

        let checkpoint_num = epoch_checkpoint.checkpoint_number;

        let path = ObjectPath::from("epochs.json");
        let store = conn.object_store();

        let (mut epochs, e_tag, version, file_exists) = match store.get(&path).await {
            Ok(result) => {
                let e_tag = result.meta.e_tag.clone();
                let version = result.meta.version.clone();
                let bytes = result.bytes().await?;
                let epochs: Vec<u64> =
                    serde_json::from_slice(&bytes).context("Failed to parse epochs.json")?;
                (epochs, e_tag, version, true)
            }
            Err(ObjectStoreError::NotFound { .. }) => (Vec::new(), None, None, false),
            Err(e) => return Err(e.into()),
        };

        match epochs.binary_search(&checkpoint_num) {
            Ok(_) => return Ok(0),
            Err(pos) => epochs.insert(pos, checkpoint_num),
        }

        let json_bytes = serde_json::to_vec(&epochs)?;
        let payload: PutPayload = Bytes::from(json_bytes).into();

        if file_exists {
            store
                .put_opts(
                    &path,
                    payload,
                    PutMode::Update(object_store::UpdateVersion { e_tag, version }).into(),
                )
                .await?;
        } else {
            store
                .put_opts(&path, payload, PutMode::Create.into())
                .await?;
        }

        Ok(1)
    }
}
