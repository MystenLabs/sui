// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Canonical in-memory `Connection` test double for store trait tests.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::bail;
use async_trait::async_trait;
use dashmap::DashMap;
use dashmap::mapref::one::Ref;
use scoped_futures::ScopedBoxFuture;

use crate::CommitterWatermark;
use crate::ConcurrentConnection;
use crate::ConcurrentStore;
use crate::Connection;
use crate::InitWatermark;
use crate::PrunerWatermark;
use crate::ReaderWatermark;
use crate::SequentialConnection;
use crate::SequentialStore;
use crate::Store;

#[derive(Default, Clone)]
pub struct MockWatermark {
    pub epoch_hi_inclusive: u64,
    // Some -> highest indexed checkpoint
    // None -> pipeline has been initialized, but no checkpoints have been indexed
    pub checkpoint_hi_inclusive: Option<u64>,
    pub tx_hi: u64,
    pub timestamp_ms_hi_inclusive: u64,
    pub reader_lo: u64,
    pub pruner_timestamp: u64,
    pub pruner_hi: u64,
    pub chain_id: Option<[u8; 32]>,
}

/// In-memory `Store`/`Connection` for exercising the shared store-trait test macros.
#[derive(Clone, Default)]
pub struct MockStore {
    /// Maps each pipeline's name to its watermark.
    pub watermarks: Arc<DashMap<String, MockWatermark>>,
}

#[derive(Clone)]
pub struct MockConnection<'c>(pub &'c MockStore);

impl MockWatermark {
    fn for_init(checkpoint_hi_inclusive: Option<u64>, reader_lo: u64) -> Self {
        let pruner_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Self {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
            reader_lo,
            pruner_timestamp,
            pruner_hi: reader_lo,
            chain_id: None,
        }
    }
}

impl MockConnection<'_> {
    async fn get_watermark(
        &self,
        pipeline: &str,
    ) -> anyhow::Result<Ref<'_, String, MockWatermark>> {
        let Some(watermark) = self.0.watermarks.get(pipeline) else {
            bail!("Pipeline {pipeline} not found");
        };
        Ok(watermark)
    }
}

#[async_trait]
impl Store for MockStore {
    type Connection<'c> = MockConnection<'c>;

    async fn connect(&self) -> anyhow::Result<Self::Connection<'_>> {
        Ok(MockConnection(self))
    }
}

#[async_trait]
impl ConcurrentStore for MockStore {
    type ConcurrentConnection<'c> = MockConnection<'c>;
}

#[async_trait]
impl SequentialStore for MockStore {
    type SequentialConnection<'c> = MockConnection<'c>;

    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(
            &'r mut Self::Connection<'_>,
        ) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>,
    {
        let snapshot: HashMap<String, MockWatermark> = self
            .watermarks
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();

        let mut conn = self.connect().await?;
        match f(&mut conn).await {
            Ok(r) => Ok(r),
            Err(e) => {
                self.watermarks.clear();
                for (k, v) in snapshot {
                    self.watermarks.insert(k, v);
                }
                Err(e)
            }
        }
    }
}

#[async_trait]
impl Connection for MockConnection<'_> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        checkpoint_hi_inclusive: Option<u64>,
    ) -> anyhow::Result<Option<InitWatermark>> {
        let watermark = self
            .0
            .watermarks
            .entry(pipeline_task.to_string())
            .or_insert_with(|| {
                MockWatermark::for_init(
                    checkpoint_hi_inclusive,
                    checkpoint_hi_inclusive.map_or(0, |c| c + 1),
                )
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
        let mut wm = self
            .0
            .watermarks
            .entry(pipeline_task.to_string())
            .or_default();

        if let Some(stored_chain_id) = wm.chain_id {
            Ok(stored_chain_id == chain_id)
        } else {
            wm.chain_id = Some(chain_id);
            Ok(true)
        }
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> Result<Option<CommitterWatermark>, anyhow::Error> {
        let watermark = self.get_watermark(pipeline_task).await?;
        let Some(checkpoint_hi_inclusive) = watermark.checkpoint_hi_inclusive else {
            return Ok(None);
        };

        Ok(Some(CommitterWatermark {
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
        let mut wm = self
            .0
            .watermarks
            .entry(pipeline_task.to_string())
            .or_default();

        if let Some(existing) = wm.checkpoint_hi_inclusive
            && watermark.checkpoint_hi_inclusive <= existing
        {
            return Ok(false);
        }

        wm.epoch_hi_inclusive = watermark.epoch_hi_inclusive;
        wm.checkpoint_hi_inclusive = Some(watermark.checkpoint_hi_inclusive);
        wm.tx_hi = watermark.tx_hi;
        wm.timestamp_ms_hi_inclusive = watermark.timestamp_ms_hi_inclusive;
        Ok(true)
    }
}

#[async_trait]
impl ConcurrentConnection for MockConnection<'_> {
    async fn reader_watermark(
        &mut self,
        pipeline: &str,
    ) -> Result<Option<ReaderWatermark>, anyhow::Error> {
        let watermark = self.get_watermark(pipeline).await?;
        let Some(checkpoint_hi_inclusive) = watermark.checkpoint_hi_inclusive else {
            return Ok(None);
        };

        Ok(Some(ReaderWatermark {
            checkpoint_hi_inclusive,
            reader_lo: watermark.reader_lo,
        }))
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> Result<Option<PrunerWatermark>, anyhow::Error> {
        let watermark = self.get_watermark(pipeline).await?;
        if watermark.checkpoint_hi_inclusive.is_none() {
            return Ok(None);
        }

        let elapsed_ms = watermark.pruner_timestamp as i64
            - SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
        let wait_for_ms = delay.as_millis() as i64 + elapsed_ms;
        Ok(Some(PrunerWatermark {
            pruner_hi: watermark.pruner_hi,
            reader_lo: watermark.reader_lo,
            wait_for_ms,
        }))
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> anyhow::Result<bool> {
        let mut curr = self.0.watermarks.get_mut(pipeline).unwrap();
        if reader_lo <= curr.reader_lo {
            return Ok(false);
        }
        curr.reader_lo = reader_lo;
        curr.pruner_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Ok(true)
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        let mut curr = self.0.watermarks.get_mut(pipeline).unwrap();
        if pruner_hi <= curr.pruner_hi {
            return Ok(false);
        }
        curr.pruner_hi = pruner_hi;
        Ok(true)
    }
}

#[async_trait]
impl SequentialConnection for MockConnection<'_> {}
