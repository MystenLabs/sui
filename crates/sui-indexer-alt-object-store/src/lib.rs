// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::Error as ObjectStoreError;
use object_store::ObjectStoreExt as _;
use object_store::PutMode;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_framework_store_traits as framework_traits;
use sui_indexer_alt_framework_store_traits::ConcurrentConnection;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::InitWatermark;
use sui_indexer_alt_framework_store_traits::PrunerWatermark;
use sui_indexer_alt_framework_store_traits::ReaderWatermark;
use sui_indexer_alt_framework_store_traits::Store;
use tracing::info;

#[derive(Clone)]
pub struct ObjectStore {
    object_store: Arc<dyn object_store::ObjectStore>,
}

pub struct ObjectStoreConnection {
    object_store: Arc<dyn object_store::ObjectStore>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct ConcurrentWatermarkData {
    reader_lo: u64,
    pruner_hi: u64,
    pruner_timestamp_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct CommitterWatermark {
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

    fn concurrent_watermark_path(pipeline: &str) -> ObjectPath {
        ObjectPath::from(format!(
            "_metadata/concurrent_watermarks/{}.json",
            pipeline
        ))
    }

    async fn get_concurrent_watermark(
        &self,
        path: &ObjectPath,
    ) -> anyhow::Result<(ConcurrentWatermarkData, Option<object_store::UpdateVersion>)> {
        match self.object_store.get(path).await {
            Ok(result) => {
                let update_version = object_store::UpdateVersion {
                    e_tag: result.meta.e_tag.clone(),
                    version: result.meta.version.clone(),
                };
                let bytes = result.bytes().await?;
                let data = serde_json::from_slice(&bytes)
                    .context("Failed to parse concurrent watermark from object store")?;
                Ok((data, Some(update_version)))
            }
            Err(ObjectStoreError::NotFound { .. }) => Ok((ConcurrentWatermarkData::default(), None)),
            Err(e) => Err(e.into()),
        }
    }

    async fn put_concurrent_watermark(
        &self,
        path: &ObjectPath,
        data: &ConcurrentWatermarkData,
        update_version: Option<object_store::UpdateVersion>,
    ) -> anyhow::Result<()> {
        let json_bytes = serde_json::to_vec(data)?;
        let payload: PutPayload = Bytes::from(json_bytes).into();
        let mode = match update_version {
            Some(v) => PutMode::Update(v),
            None => PutMode::Create,
        };
        self.object_store
            .put_opts(path, payload, mode.into())
            .await?;
        Ok(())
    }
}

impl From<framework_traits::CommitterWatermark> for CommitterWatermark {
    fn from(w: framework_traits::CommitterWatermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}

impl From<CommitterWatermark> for framework_traits::CommitterWatermark {
    fn from(w: CommitterWatermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}

#[async_trait]
impl framework_traits::ConcurrentStore for ObjectStore {
    type ConcurrentConnection<'c> = ObjectStoreConnection;
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
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        checkpoint_hi_inclusive: Option<u64>,
    ) -> anyhow::Result<Option<InitWatermark>> {
        let path = Self::concurrent_watermark_path(pipeline_task);

        let reader_lo = checkpoint_hi_inclusive.map_or(0, |c| c + 1);
        let data = ConcurrentWatermarkData {
            reader_lo,
            pruner_hi: reader_lo,
            ..Default::default()
        };

        let json_bytes = serde_json::to_vec(&data)?;
        let payload: PutPayload = Bytes::from(json_bytes).into();
        match self
            .object_store
            .put_opts(&path, payload, PutMode::Create.into())
            .await
        {
            Ok(_) => Ok(Some(InitWatermark {
                checkpoint_hi_inclusive,
                reader_lo: Some(reader_lo),
            })),
            Err(ObjectStoreError::AlreadyExists { .. }) => {
                let (data, _) = self.get_concurrent_watermark(&path).await?;

                let committer_checkpoint = self
                    .committer_watermark(pipeline_task)
                    .await?
                    .map(|w| w.checkpoint_hi_inclusive);

                Ok(Some(InitWatermark {
                    checkpoint_hi_inclusive: committer_checkpoint,
                    reader_lo: Some(data.reader_lo),
                }))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn accepts_chain_id(
        &mut self,
        _pipeline_task: &str,
        _chain_id: [u8; 32],
    ) -> anyhow::Result<bool> {
        // TODO: Implement storing chain_id
        Ok(true)
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> anyhow::Result<Option<framework_traits::CommitterWatermark>> {
        let object_path = ObjectPath::from(format!("_metadata/watermarks/{}.json", pipeline_task));
        match self.object_store.get(&object_path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                let watermark: CommitterWatermark = serde_json::from_slice(&bytes)
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

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: framework_traits::CommitterWatermark,
    ) -> anyhow::Result<bool> {
        let new_watermark: CommitterWatermark = watermark.into();
        let object_path = ObjectPath::from(format!("_metadata/watermarks/{}.json", pipeline_task));

        let (current_watermark, e_tag, version) = match self.object_store.get(&object_path).await {
            Ok(result) => {
                let e_tag = result.meta.e_tag.clone();
                let version = result.meta.version.clone();
                let bytes = result.bytes().await?;
                let watermark: CommitterWatermark = serde_json::from_slice(&bytes)
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
}

#[async_trait]
impl ConcurrentConnection for ObjectStoreConnection {
    async fn reader_watermark(
        &mut self,
        pipeline: &str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        let Some(committer) = self.committer_watermark(pipeline).await? else {
            return Ok(None);
        };

        let path = Self::concurrent_watermark_path(pipeline);
        let (data, _) = self.get_concurrent_watermark(&path).await?;

        Ok(Some(ReaderWatermark {
            checkpoint_hi_inclusive: committer.checkpoint_hi_inclusive,
            reader_lo: data.reader_lo,
        }))
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        if self.committer_watermark(pipeline).await?.is_none() {
            return Ok(None);
        }

        let path = Self::concurrent_watermark_path(pipeline);
        let (data, _) = self.get_concurrent_watermark(&path).await?;

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let wait_for_ms = delay.as_millis() as i64 + data.pruner_timestamp_ms as i64 - now_ms;

        Ok(Some(PrunerWatermark {
            wait_for_ms,
            reader_lo: data.reader_lo,
            pruner_hi: data.pruner_hi,
        }))
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> anyhow::Result<bool> {
        let path = Self::concurrent_watermark_path(pipeline);
        let (mut data, update_version) = self.get_concurrent_watermark(&path).await?;

        if data.reader_lo >= reader_lo {
            return Ok(false);
        }

        data.reader_lo = reader_lo;
        data.pruner_timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        self.put_concurrent_watermark(&path, &data, update_version)
            .await?;
        Ok(true)
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        let path = Self::concurrent_watermark_path(pipeline);
        let (mut data, update_version) = self.get_concurrent_watermark(&path).await?;

        if data.pruner_hi >= pruner_hi {
            return Ok(false);
        }

        data.pruner_hi = pruner_hi;

        self.put_concurrent_watermark(&path, &data, update_version)
            .await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use object_store::memory::InMemory;

    use super::*;

    fn committer_watermark(checkpoint: u64) -> framework_traits::CommitterWatermark {
        framework_traits::CommitterWatermark {
            epoch_hi_inclusive: 1,
            checkpoint_hi_inclusive: checkpoint,
            tx_hi: 500,
            timestamp_ms_hi_inclusive: 1000,
        }
    }

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

    #[tokio::test]
    async fn test_reader_watermark_none_without_committer() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        let result = conn.reader_watermark("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_reader_watermark_defaults_reader_lo_to_zero() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_committer_watermark("pipeline", committer_watermark(100))
            .await
            .unwrap();

        let rw = conn.reader_watermark("pipeline").await.unwrap().unwrap();
        assert_eq!(rw.checkpoint_hi_inclusive, 100);
        assert_eq!(rw.reader_lo, 0);
    }

    #[tokio::test]
    async fn test_set_and_get_reader_watermark() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_committer_watermark("pipeline", committer_watermark(200))
            .await
            .unwrap();

        let updated = conn.set_reader_watermark("pipeline", 50).await.unwrap();
        assert!(updated);

        let rw = conn.reader_watermark("pipeline").await.unwrap().unwrap();
        assert_eq!(rw.checkpoint_hi_inclusive, 200);
        assert_eq!(rw.reader_lo, 50);
    }

    #[tokio::test]
    async fn test_set_reader_watermark_rejects_regression() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_reader_watermark("pipeline", 100).await.unwrap();

        let updated = conn.set_reader_watermark("pipeline", 50).await.unwrap();
        assert!(!updated);

        let updated = conn.set_reader_watermark("pipeline", 100).await.unwrap();
        assert!(!updated);
    }

    #[tokio::test]
    async fn test_set_reader_watermark_raises() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_reader_watermark("pipeline", 100).await.unwrap();

        let updated = conn.set_reader_watermark("pipeline", 200).await.unwrap();
        assert!(updated);

        conn.set_committer_watermark("pipeline", committer_watermark(300))
            .await
            .unwrap();

        let rw = conn.reader_watermark("pipeline").await.unwrap().unwrap();
        assert_eq!(rw.reader_lo, 200);
    }

    #[tokio::test]
    async fn test_set_and_get_pruner_watermark() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_committer_watermark("pipeline", committer_watermark(200))
            .await
            .unwrap();

        let updated = conn.set_pruner_watermark("pipeline", 75).await.unwrap();
        assert!(updated);

        let pw = conn
            .pruner_watermark("pipeline", Duration::ZERO)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pw.pruner_hi, 75);
        assert_eq!(pw.reader_lo, 0);
    }

    #[tokio::test]
    async fn test_set_pruner_watermark_rejects_regression() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_pruner_watermark("pipeline", 100).await.unwrap();

        let updated = conn.set_pruner_watermark("pipeline", 50).await.unwrap();
        assert!(!updated);

        let updated = conn.set_pruner_watermark("pipeline", 100).await.unwrap();
        assert!(!updated);
    }

    #[tokio::test]
    async fn test_pruner_watermark_none_without_committer() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        let result = conn
            .pruner_watermark("nonexistent", Duration::ZERO)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_pruner_watermark_wait_for_with_delay() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_committer_watermark("pipeline", committer_watermark(200))
            .await
            .unwrap();

        // set_reader_watermark sets the pruner_timestamp to now
        conn.set_reader_watermark("pipeline", 100).await.unwrap();

        let delay = Duration::from_secs(60);
        let pw = conn
            .pruner_watermark("pipeline", delay)
            .await
            .unwrap()
            .unwrap();

        // wait_for_ms should be approximately 60_000 (delay) since pruner_timestamp was just set
        assert!(pw.wait_for_ms > 59_000 && pw.wait_for_ms <= 60_000);
        assert_eq!(pw.reader_lo, 100);
        assert_eq!(pw.pruner_hi, 0);
    }

    #[tokio::test]
    async fn test_pruner_watermark_zero_delay_negative_wait() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_committer_watermark("pipeline", committer_watermark(200))
            .await
            .unwrap();

        conn.set_reader_watermark("pipeline", 100).await.unwrap();

        let pw = conn
            .pruner_watermark("pipeline", Duration::ZERO)
            .await
            .unwrap()
            .unwrap();

        // With zero delay and pruner_timestamp ~ now, wait_for_ms should be <= 0
        assert!(pw.wait_for_ms <= 0);
    }

    #[tokio::test]
    async fn test_concurrent_watermark_independent_pipelines() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_reader_watermark("pipeline_a", 100).await.unwrap();
        conn.set_reader_watermark("pipeline_b", 200).await.unwrap();
        conn.set_pruner_watermark("pipeline_a", 50).await.unwrap();

        for pipeline in ["pipeline_a", "pipeline_b"] {
            conn.set_committer_watermark(pipeline, committer_watermark(300))
                .await
                .unwrap();
        }

        let rw_a = conn.reader_watermark("pipeline_a").await.unwrap().unwrap();
        assert_eq!(rw_a.reader_lo, 100);

        let rw_b = conn.reader_watermark("pipeline_b").await.unwrap().unwrap();
        assert_eq!(rw_b.reader_lo, 200);

        let pw_a = conn
            .pruner_watermark("pipeline_a", Duration::ZERO)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pw_a.pruner_hi, 50);

        let pw_b = conn
            .pruner_watermark("pipeline_b", Duration::ZERO)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pw_b.pruner_hi, 0);
    }

    #[tokio::test]
    async fn test_init_watermark_creates_concurrent_watermark() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        let init = conn
            .init_watermark("pipeline", Some(99))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, Some(99));
        assert_eq!(init.reader_lo, Some(100));
    }

    #[tokio::test]
    async fn test_init_watermark_creates_with_none_checkpoint() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        let init = conn
            .init_watermark("pipeline", None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, None);
        assert_eq!(init.reader_lo, Some(0));
    }

    #[tokio::test]
    async fn test_init_watermark_does_not_overwrite_existing() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.set_committer_watermark("pipeline", committer_watermark(200))
            .await
            .unwrap();
        conn.set_reader_watermark("pipeline", 30).await.unwrap();

        // Calling init_watermark again should return existing values, not overwrite
        let init = conn
            .init_watermark("pipeline", Some(999))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, Some(200));
        assert_eq!(init.reader_lo, Some(30));
    }

    #[tokio::test]
    async fn test_init_watermark_sets_pruner_hi_equal_to_reader_lo() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        conn.init_watermark("pipeline", Some(99)).await.unwrap();

        conn.set_committer_watermark("pipeline", committer_watermark(200))
            .await
            .unwrap();

        let pw = conn
            .pruner_watermark("pipeline", Duration::ZERO)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pw.pruner_hi, 100);
        assert_eq!(pw.reader_lo, 100);
    }
}
