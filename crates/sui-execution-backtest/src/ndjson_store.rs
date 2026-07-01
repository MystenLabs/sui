// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A zero-setup `ConcurrentStore` that appends rows to a newline-delimited JSON file instead of a
//! database. It lets the same `Indexer`/`Handler` pipeline run without a postgres dependency (for
//! quick local smoke tests); the postgres path is the durable, queryable one.
//!
//! Watermarks are kept in memory only, so an ndjson run does not resume across process restarts —
//! acceptable for the bounded smoke runs this sink is for. Persisting them to a sidecar file (to
//! match the postgres path's resumability) is a deliberate non-goal for now.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context as _;
use anyhow::bail;
use async_trait::async_trait;
use serde::Serialize;
use sui_indexer_alt_framework::store::CommitterWatermark;
use sui_indexer_alt_framework::store::ConcurrentConnection;
use sui_indexer_alt_framework::store::ConcurrentStore;
use sui_indexer_alt_framework::store::Connection;
use sui_indexer_alt_framework::store::InitWatermark;
use sui_indexer_alt_framework::store::PrunerWatermark;
use sui_indexer_alt_framework::store::ReaderWatermark;
use sui_indexer_alt_framework::store::Store;

/// In-memory mirror of a pipeline's watermark row.
#[derive(Default, Clone)]
struct Watermark {
    epoch_hi_inclusive: u64,
    /// `Some` once a checkpoint has been committed; `None` after init but before any data.
    checkpoint_hi_inclusive: Option<u64>,
    tx_hi: u64,
    timestamp_ms_hi_inclusive: u64,
    reader_lo: u64,
    pruner_timestamp: u64,
    pruner_hi: u64,
    chain_id: Option<[u8; 32]>,
}

/// Appends rows to a newline-delimited JSON file; watermarks live in memory.
#[derive(Clone)]
pub struct NdjsonStore {
    writer: Arc<Mutex<BufWriter<File>>>,
    watermarks: Arc<Mutex<HashMap<String, Watermark>>>,
}

pub struct NdjsonConnection<'c>(&'c NdjsonStore);

impl NdjsonConnection<'_> {
    /// The store backing this connection, so a handler's `commit` can append rows to the file.
    pub(crate) fn store(&self) -> &NdjsonStore {
        self.0
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl NdjsonStore {
    /// Create (truncating) the output file at `path`.
    pub fn create(path: &Path) -> anyhow::Result<Self> {
        let file = File::create(path)
            .with_context(|| format!("creating output file {}", path.display()))?;
        Ok(Self {
            writer: Arc::new(Mutex::new(BufWriter::new(file))),
            watermarks: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Append `rows` to the file, one JSON object per line. Called by the handler's `commit`.
    pub fn write_rows<T: Serialize>(&self, rows: &[T]) -> anyhow::Result<usize> {
        let mut writer = self.writer.lock().unwrap();
        for row in rows {
            serde_json::to_writer(&mut *writer, row).context("serializing row")?;
            writer.write_all(b"\n").context("writing row")?;
        }
        Ok(rows.len())
    }

    /// Flush buffered output to disk.
    pub fn flush(&self) -> anyhow::Result<()> {
        self.writer
            .lock()
            .unwrap()
            .flush()
            .context("flushing output")
    }
}

#[async_trait]
impl Store for NdjsonStore {
    type Connection<'c> = NdjsonConnection<'c>;

    async fn connect(&self) -> anyhow::Result<Self::Connection<'_>> {
        Ok(NdjsonConnection(self))
    }
}

#[async_trait]
impl ConcurrentStore for NdjsonStore {
    type ConcurrentConnection<'c> = NdjsonConnection<'c>;
}

#[async_trait]
impl Connection for NdjsonConnection<'_> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        checkpoint_hi_inclusive: Option<u64>,
    ) -> anyhow::Result<Option<InitWatermark>> {
        let mut watermarks = self.0.watermarks.lock().unwrap();
        let watermark = watermarks
            .entry(pipeline_task.to_owned())
            .or_insert_with(|| Watermark {
                checkpoint_hi_inclusive,
                reader_lo: checkpoint_hi_inclusive.map_or(0, |c| c + 1),
                pruner_timestamp: now_ms(),
                pruner_hi: checkpoint_hi_inclusive.map_or(0, |c| c + 1),
                ..Default::default()
            });
        Ok(Some(InitWatermark {
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
            reader_lo: Some(watermark.reader_lo),
        }))
    }

    async fn accepts_chain_id(
        &mut self,
        pipeline_task: &str,
        chain_id: [u8; 32],
    ) -> anyhow::Result<bool> {
        let mut watermarks = self.0.watermarks.lock().unwrap();
        let watermark = watermarks.entry(pipeline_task.to_owned()).or_default();
        match watermark.chain_id {
            Some(stored) => Ok(stored == chain_id),
            None => {
                watermark.chain_id = Some(chain_id);
                Ok(true)
            }
        }
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        let watermarks = self.0.watermarks.lock().unwrap();
        let Some(watermark) = watermarks.get(pipeline_task) else {
            bail!("pipeline {pipeline_task} not found");
        };
        Ok(watermark
            .checkpoint_hi_inclusive
            .map(|checkpoint_hi_inclusive| CommitterWatermark {
                epoch_hi_inclusive: watermark.epoch_hi_inclusive,
                checkpoint_hi_inclusive,
                tx_hi: watermark.tx_hi,
                timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
            }))
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        let mut watermarks = self.0.watermarks.lock().unwrap();
        let entry = watermarks.entry(pipeline_task.to_owned()).or_default();
        // Watermarks must not regress: an equal or lower write is stale.
        if let Some(existing) = entry.checkpoint_hi_inclusive
            && watermark.checkpoint_hi_inclusive <= existing
        {
            return Ok(false);
        }
        entry.epoch_hi_inclusive = watermark.epoch_hi_inclusive;
        entry.checkpoint_hi_inclusive = Some(watermark.checkpoint_hi_inclusive);
        entry.tx_hi = watermark.tx_hi;
        entry.timestamp_ms_hi_inclusive = watermark.timestamp_ms_hi_inclusive;
        Ok(true)
    }
}

#[async_trait]
impl ConcurrentConnection for NdjsonConnection<'_> {
    async fn reader_watermark(
        &mut self,
        pipeline: &str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        let watermarks = self.0.watermarks.lock().unwrap();
        // A missing entry means "no reader watermark" (the framework reads this for the main
        // pipeline, which is absent when only a tasked pipeline runs). Returning `Ok(None)` is the
        // contract — bailing would stall the tasked pipeline's collector, which waits on it.
        let Some(watermark) = watermarks.get(pipeline) else {
            return Ok(None);
        };
        Ok(watermark
            .checkpoint_hi_inclusive
            .map(|checkpoint_hi_inclusive| ReaderWatermark {
                checkpoint_hi_inclusive,
                reader_lo: watermark.reader_lo,
            }))
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        let watermarks = self.0.watermarks.lock().unwrap();
        let Some(watermark) = watermarks.get(pipeline) else {
            return Ok(None);
        };
        if watermark.checkpoint_hi_inclusive.is_none() {
            return Ok(None);
        }
        let elapsed_ms = watermark.pruner_timestamp as i64 - now_ms() as i64;
        Ok(Some(PrunerWatermark {
            pruner_hi: watermark.pruner_hi,
            reader_lo: watermark.reader_lo,
            wait_for_ms: delay.as_millis() as i64 + elapsed_ms,
        }))
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> anyhow::Result<bool> {
        let mut watermarks = self.0.watermarks.lock().unwrap();
        let Some(watermark) = watermarks.get_mut(pipeline) else {
            return Ok(false);
        };
        if reader_lo <= watermark.reader_lo {
            return Ok(false);
        }
        watermark.reader_lo = reader_lo;
        watermark.pruner_timestamp = now_ms();
        Ok(true)
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        let mut watermarks = self.0.watermarks.lock().unwrap();
        let Some(watermark) = watermarks.get_mut(pipeline) else {
            return Ok(false);
        };
        if pruner_hi <= watermark.pruner_hi {
            return Ok(false);
        }
        watermark.pruner_hi = pruner_hi;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_rows_and_tracks_watermark() {
        let path = std::env::temp_dir().join("backtest_ndjson_store_test.ndjson");
        let store = NdjsonStore::create(&path).unwrap();

        store
            .write_rows(&[
                serde_json::json!({"digest": "a"}),
                serde_json::json!({"digest": "b"}),
            ])
            .unwrap();
        store.flush().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 2);

        let mut conn = store.connect().await.unwrap();
        // Unknown pipeline errors; after init it exists but has no committed checkpoint yet.
        assert!(conn.committer_watermark("bt@run1").await.is_err());
        conn.init_watermark("bt@run1", None).await.unwrap();
        assert!(conn.committer_watermark("bt@run1").await.unwrap().is_none());

        let wm = CommitterWatermark {
            epoch_hi_inclusive: 1,
            checkpoint_hi_inclusive: 100,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
        };
        assert!(conn.set_committer_watermark("bt@run1", wm).await.unwrap());
        // A non-advancing write is stale and rejected.
        assert!(!conn.set_committer_watermark("bt@run1", wm).await.unwrap());
        assert_eq!(
            conn.committer_watermark("bt@run1")
                .await
                .unwrap()
                .unwrap()
                .checkpoint_hi_inclusive,
            100
        );

        std::fs::remove_file(&path).ok();
    }
}
