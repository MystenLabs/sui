use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::path::Path as StorePath;
use serde::{Deserialize, Serialize};
use sui_indexer_alt_framework_store_traits::{
    CommitterWatermark, Connection, PrunerWatermark, ReaderWatermark, Store,
};

#[derive(Clone)]
pub struct ObjectStore {
    object_store: Arc<Box<dyn object_store::ObjectStore>>,
    compression_level: Option<i32>,
}

pub struct ObjectStoreConnection<'c> {
    store: &'c ObjectStore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCommitterWatermark {
    epoch_hi_inclusive: u64,
    checkpoint_hi_inclusive: u64,
    tx_hi: u64,
    timestamp_ms_hi_inclusive: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredReaderWatermark {
    checkpoint_hi_inclusive: u64,
    reader_lo: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredPrunerWatermark {
    reader_lo: u64,
    pruner_hi: u64,
}

impl From<CommitterWatermark> for StoredCommitterWatermark {
    fn from(w: CommitterWatermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}

fn watermark_path(pipeline: &str, watermark_type: &str) -> StorePath {
    StorePath::from(format!("_watermarks/{}/{}.json", pipeline, watermark_type))
}

impl ObjectStore {
    pub fn new(
        object_store: Box<dyn object_store::ObjectStore>,
        compression_level: Option<i32>,
    ) -> Self {
        Self {
            object_store: Arc::new(object_store),
            compression_level,
        }
    }
}

impl ObjectStoreConnection<'_> {
    pub fn object_store(&self) -> &Arc<Box<dyn object_store::ObjectStore>> {
        &self.store.object_store
    }

    pub async fn write(&self, path: impl Into<StorePath>, data: impl AsRef<[u8]>) -> Result<()> {
        let mut path = path.into();

        let blob: object_store::PutPayload = if let Some(level) = self.store.compression_level {
            let path_str = format!("{}.zst", path);
            path = path_str.into();

            let compressed = tokio::task::spawn_blocking({
                let data = data.as_ref().to_vec();
                move || zstd::encode_all(&data[..], level)
            })
            .await??;

            Bytes::from(compressed).into()
        } else {
            Bytes::from(data.as_ref().to_vec()).into()
        };

        self.store.object_store.put(&path, blob).await?;
        Ok(())
    }
}

#[async_trait]
impl Connection for ObjectStoreConnection<'_> {
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> Result<Option<CommitterWatermark>> {
        let path = watermark_path(pipeline, "committer");

        match self.store.object_store.get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                let stored: StoredCommitterWatermark = serde_json::from_slice(&bytes)?;
                Ok(Some(CommitterWatermark {
                    epoch_hi_inclusive: stored.epoch_hi_inclusive,
                    checkpoint_hi_inclusive: stored.checkpoint_hi_inclusive,
                    tx_hi: stored.tx_hi,
                    timestamp_ms_hi_inclusive: stored.timestamp_ms_hi_inclusive,
                }))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> Result<Option<ReaderWatermark>> {
        let path = watermark_path(pipeline, "reader");

        match self.store.object_store.get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                let stored: StoredReaderWatermark = serde_json::from_slice(&bytes)?;
                Ok(Some(ReaderWatermark {
                    checkpoint_hi_inclusive: stored.checkpoint_hi_inclusive,
                    reader_lo: stored.reader_lo,
                }))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        _delay: Duration,
    ) -> Result<Option<PrunerWatermark>> {
        let path = watermark_path(pipeline, "pruner");

        match self.store.object_store.get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                let stored: StoredPrunerWatermark = serde_json::from_slice(&bytes)?;
                Ok(Some(PrunerWatermark {
                    wait_for_ms: 0,
                    reader_lo: stored.reader_lo,
                    pruner_hi: stored.pruner_hi,
                }))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> Result<bool> {
        let path = watermark_path(pipeline, "committer");
        let stored = StoredCommitterWatermark::from(watermark);
        let json = serde_json::to_vec_pretty(&stored)?;

        self.store
            .object_store
            .put(&path, Bytes::from(json).into())
            .await?;

        Ok(true)
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> Result<bool> {
        let path = watermark_path(pipeline, "reader");

        // Need to read to get checkpoint_hi_inclusive which is set separately
        let checkpoint_hi_inclusive = match self.store.object_store.get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                let existing: StoredReaderWatermark = serde_json::from_slice(&bytes)?;
                existing.checkpoint_hi_inclusive
            }
            Err(object_store::Error::NotFound { .. }) => 0,
            Err(e) => return Err(e.into()),
        };

        let stored = StoredReaderWatermark {
            checkpoint_hi_inclusive,
            reader_lo,
        };
        let json = serde_json::to_vec_pretty(&stored)?;

        self.store
            .object_store
            .put(&path, Bytes::from(json).into())
            .await?;

        Ok(true)
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> Result<bool> {
        let path = watermark_path(pipeline, "pruner");

        // Read existing to get reader_lo
        let reader_lo = match self.store.object_store.get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                let existing: StoredPrunerWatermark = serde_json::from_slice(&bytes)?;
                existing.reader_lo
            }
            Err(object_store::Error::NotFound { .. }) => 0,
            Err(e) => return Err(e.into()),
        };

        let stored = StoredPrunerWatermark {
            reader_lo,
            pruner_hi,
        };
        let json = serde_json::to_vec_pretty(&stored)?;
        self.store
            .object_store
            .put(&path, Bytes::from(json).into())
            .await?;
        Ok(true)
    }
}

#[async_trait]
impl Store for ObjectStore {
    type Connection<'c> = ObjectStoreConnection<'c>;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>> {
        Ok(ObjectStoreConnection { store: self })
    }
}
