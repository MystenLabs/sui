// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use async_trait::async_trait;
use bytes::Bytes;
use object_store::path::Path as ObjectPath;
use object_store::{Error as ObjectStoreError, PutMode, PutPayload};
use serde::{Deserialize, Serialize};
use sui_indexer_alt_framework_store_traits::{
    self as framework_traits, Connection, PrunerWatermark, ReaderWatermark, Store,
};
use tracing::info;

#[derive(Clone)]
pub struct ObjectStore {
    object_store: Arc<dyn object_store::ObjectStore>,
}

pub struct ObjectStoreConnection {
    object_store: Arc<dyn object_store::ObjectStore>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ComitterWatermark {
    epoch_hi_inclusive: u64,
    checkpoint_hi_inclusive: u64,
    tx_hi: u64,
    timestamp_ms_hi_inclusive: u64,
}

impl ObjectStore {
    pub fn new(object_store: Arc<dyn object_store::ObjectStore>) -> Self {
        Self { object_store }
    }
}

impl ObjectStoreConnection {
    pub fn object_store(&self) -> Arc<dyn object_store::ObjectStore> {
        self.object_store.clone()
    }
}

impl From<framework_traits::CommitterWatermark> for ComitterWatermark {
    fn from(w: framework_traits::CommitterWatermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}

impl From<ComitterWatermark> for framework_traits::CommitterWatermark {
    fn from(w: ComitterWatermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}

#[async_trait]
impl Store for ObjectStore {
    type Connection<'c> = ObjectStoreConnection;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        Ok(ObjectStoreConnection {
            object_store: self.object_store.clone(),
        })
    }
}

#[async_trait]
impl Connection for ObjectStoreConnection {
    async fn init_watermark(&mut self, pipeline_task: &str, _: u64) -> anyhow::Result<Option<u64>> {
        Ok(self
            .committer_watermark(pipeline_task)
            .await?
            .map(|w| w.checkpoint_hi_inclusive))
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> anyhow::Result<Option<framework_traits::CommitterWatermark>> {
        let object_path = ObjectPath::from(format!("_metadata/watermarks/{}.json", pipeline_task));
        match self.object_store.get(&object_path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                let watermark: ComitterWatermark = serde_json::from_slice(&bytes)
                    .context("Failed to parse watermark from object store")?;

                info!(
                    pipeline_task,
                    checkpoint = watermark.checkpoint_hi_inclusive,
                    "Downloaded watermark from object store"
                );

                Ok(Some(watermark.into()))
            }
            Err(ObjectStoreError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn reader_watermark(
        &mut self,
        _pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        Ok(None)
    }

    async fn pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        Ok(None)
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: framework_traits::CommitterWatermark,
    ) -> anyhow::Result<bool> {
        let new_watermark: ComitterWatermark = watermark.into();
        let object_path = ObjectPath::from(format!("_metadata/watermarks/{}.json", pipeline_task));

        let (current_watermark, e_tag, version) = match self.object_store.get(&object_path).await {
            Ok(result) => {
                let e_tag = result.meta.e_tag.clone();
                let version = result.meta.version.clone();
                let bytes = result.bytes().await?;
                let watermark: ComitterWatermark = serde_json::from_slice(&bytes)
                    .context("Failed to parse watermark from object store")?;
                (Some(watermark), e_tag, version)
            }
            Err(ObjectStoreError::NotFound { .. }) => (None, None, None),
            Err(e) => return Err(e.into()),
        };

        if let Some(ref current) = current_watermark
            && current.checkpoint_hi_inclusive >= new_watermark.checkpoint_hi_inclusive
        {
            return Ok(false);
        }

        let json_bytes = serde_json::to_vec(&new_watermark)?;
        let payload: PutPayload = Bytes::from(json_bytes).into();

        if current_watermark.is_some() {
            self.object_store
                .put_opts(
                    &object_path,
                    payload,
                    PutMode::Update(object_store::UpdateVersion { e_tag, version }).into(),
                )
                .await?;
        } else {
            self.object_store
                .put_opts(&object_path, payload, PutMode::Create.into())
                .await?;
        }

        Ok(true)
    }

    async fn set_reader_watermark(
        &mut self,
        _pipeline: &'static str,
        _reader_lo: u64,
    ) -> anyhow::Result<bool> {
        bail!("Pruning not supported by this store");
    }

    async fn set_pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        bail!("Pruning not supported by this store");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    #[tokio::test]
    async fn test_watermark_operations() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        let pipeline = "test_pipeline";

        // Initially, watermark should not exist
        let watermark = conn.committer_watermark(pipeline).await.unwrap();
        assert!(watermark.is_none());

        // Set initial watermark
        let initial_watermark = framework_traits::CommitterWatermark {
            epoch_hi_inclusive: 1,
            checkpoint_hi_inclusive: 100,
            tx_hi: 1000,
            timestamp_ms_hi_inclusive: 1000000,
        };
        let result = conn
            .set_committer_watermark(pipeline, initial_watermark)
            .await
            .unwrap();
        assert!(result, "First watermark update should succeed");

        // Get the watermark and verify it matches
        let watermark = conn.committer_watermark(pipeline).await.unwrap();
        assert!(watermark.is_some());
        let watermark = watermark.unwrap();
        assert_eq!(watermark.epoch_hi_inclusive, 1);
        assert_eq!(watermark.checkpoint_hi_inclusive, 100);
        assert_eq!(watermark.tx_hi, 1000);
        assert_eq!(watermark.timestamp_ms_hi_inclusive, 1000000);

        // Update watermark with higher checkpoint
        let updated_watermark = framework_traits::CommitterWatermark {
            epoch_hi_inclusive: 2,
            checkpoint_hi_inclusive: 200,
            tx_hi: 2000,
            timestamp_ms_hi_inclusive: 2000000,
        };
        let result = conn
            .set_committer_watermark(pipeline, updated_watermark)
            .await
            .unwrap();
        assert!(
            result,
            "Watermark update with higher checkpoint should succeed"
        );

        // Verify the updated watermark
        let watermark = conn.committer_watermark(pipeline).await.unwrap().unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 200);

        // Try to set a watermark with a lower checkpoint (should be rejected)
        let regressed_watermark = framework_traits::CommitterWatermark {
            epoch_hi_inclusive: 1,
            checkpoint_hi_inclusive: 150,
            tx_hi: 1500,
            timestamp_ms_hi_inclusive: 1500000,
        };
        let result = conn
            .set_committer_watermark(pipeline, regressed_watermark)
            .await
            .unwrap();
        assert!(!result, "Watermark regression should be rejected");

        // Verify watermark hasn't changed
        let watermark = conn.committer_watermark(pipeline).await.unwrap().unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 200);
    }
}
