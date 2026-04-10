// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! BigTable Store implementation for sui-indexer-alt-framework.
//!
//! Implements the `Store`, `ConcurrentStore`, and `SequentialStore` traits.
//! Per-pipeline watermarks are stored in the `watermark_alt` table.
//!
//! ## Sequential transactions
//!
//! BigTable has no multi-row transaction, so [`SequentialStore::transaction`]
//! runs the closure inline and defers the watermark write until the closure
//! returns successfully. `set_committer_watermark` buffers its write on the
//! connection; the transaction impl flushes it to BigTable after the handler's
//! commit.
//!
//! This relies on the handler's writes being idempotent on replay — for the
//! bitmap pipelines that use this, OR-based bitmap writes with monotonic
//! cumulative state are idempotent under retry. On restart, bitmap pipelines
//! resume at the start of their currently-active bucket (`init_watermark`
//! clamps via the persisted `bitmap_bucket_start_cp` column) so the partial
//! bucket is re-ingested from scratch — no straddler reconciliation needed.

use std::time::Duration;

use anyhow::Result;
use anyhow::bail;
use async_trait::async_trait;
use scoped_futures::ScopedBoxFuture;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::ConcurrentConnection;
use sui_indexer_alt_framework_store_traits::ConcurrentStore;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::InitWatermark;
use sui_indexer_alt_framework_store_traits::PrunerWatermark;
use sui_indexer_alt_framework_store_traits::ReaderWatermark;
use sui_indexer_alt_framework_store_traits::SequentialConnection;
use sui_indexer_alt_framework_store_traits::SequentialStore;
use sui_indexer_alt_framework_store_traits::Store;
use tracing::warn;

use crate::Watermark;
use crate::bigtable::client::BigTableClient;

/// A Store implementation backed by BigTable.
#[derive(Clone)]
pub struct BigTableStore {
    client: BigTableClient,
}

/// A connection to BigTable for watermark operations and data writes.
///
/// While a [`SequentialStore::transaction`] is in flight,
/// `set_committer_watermark` buffers its write in `pending_watermark` instead
/// of hitting BigTable; the transaction impl flushes it after the handler's
/// commit returns successfully.
pub struct BigTableConnection<'a> {
    client: BigTableClient,
    pending_watermark: Option<(String, CommitterWatermark)>,
    /// `true` while running under `SequentialStore::transaction`.
    in_sequential_transaction: bool,
    /// When `true`, the deferred watermark write inside
    /// [`SequentialStore::transaction`] is skipped even if `pending_watermark`
    /// is staged. Handlers set this via [`Self::skip_pending_watermark`] when
    /// they determine no durable progress needs recording (e.g. bitmap
    /// backfill mode when the batch didn't seal any bucket).
    skip_watermark_write: bool,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl BigTableStore {
    pub fn new(client: BigTableClient) -> Self {
        Self { client }
    }
}

impl BigTableConnection<'_> {
    /// Returns a mutable reference to the underlying BigTable client.
    pub fn client(&mut self) -> &mut BigTableClient {
        &mut self.client
    }

    /// Returns the watermark most recently staged by `set_committer_watermark`
    /// inside the current `SequentialStore::transaction`. Used by sequential
    /// handlers that need the about-to-be-persisted watermark in `commit()`.
    pub fn pending_watermark(&self) -> Option<CommitterWatermark> {
        self.pending_watermark.as_ref().map(|(_, w)| *w)
    }

    /// Cancel the staged committer-watermark write for the current
    /// [`SequentialStore::transaction`]. The handler can still observe the
    /// staged value via [`Self::pending_watermark`] (needed for seal detection
    /// in backfill mode), but no BigTable write is performed when the
    /// transaction closes. On restart the pipeline resumes from the
    /// previously-persisted watermark — safe because bitmap OR writes are
    /// idempotent and `init_watermark` clamps back to the start of the
    /// currently-active bucket so the partial bucket is replayed intact.
    pub fn skip_pending_watermark(&mut self) {
        self.skip_watermark_write = true;
    }
}

#[async_trait]
impl ConcurrentStore for BigTableStore {
    type ConcurrentConnection<'c> = BigTableConnection<'c>;
}

#[async_trait]
impl SequentialStore for BigTableStore {
    type SequentialConnection<'c> = BigTableConnection<'c>;

    async fn transaction<'a, R, F>(&self, f: F) -> Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(&'r mut Self::Connection<'_>) -> ScopedBoxFuture<'a, 'r, Result<R>>,
    {
        let mut conn = self.connect().await?;
        conn.in_sequential_transaction = true;
        let result = f(&mut conn).await?;
        // Closure returned `Ok` — now persist the staged watermark unless the
        // handler suppressed it via `skip_pending_watermark`. If this fails,
        // the framework retries the whole closure; the handler's
        // write-and-merge is idempotent so retries converge.
        if let Some((pipeline, watermark)) = conn.pending_watermark.take()
            && !conn.skip_watermark_write
        {
            let pw: Watermark = watermark.into();
            conn.client
                .set_pipeline_watermark(&pipeline, &pw, None)
                .await?;
        }
        Ok(result)
    }
}

#[async_trait]
impl Store for BigTableStore {
    type Connection<'c> = BigTableConnection<'c>;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>> {
        Ok(BigTableConnection {
            client: self.client.clone(),
            pending_watermark: None,
            in_sequential_transaction: false,
            skip_watermark_write: false,
            _marker: std::marker::PhantomData,
        })
    }
}

#[async_trait]
impl Connection for BigTableConnection<'_> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        _checkpoint_hi_inclusive: Option<u64>,
    ) -> Result<Option<InitWatermark>> {
        let Some(watermark) = self.committer_watermark(pipeline_task).await? else {
            return Ok(None);
        };
        // Bitmap-index pipelines persist `bitmap_bucket_start_cp` — the
        // `checkpoint_hi_inclusive` of the commit that first put a
        // transaction into the currently-active bucket. Clamping the
        // returned watermark to `cp - 1` causes the framework to resume
        // at the start of that bucket, so in-memory bitmap state is
        // rebuilt from scratch and the on-disk cells are overwritten
        // with cumulative supersets. This replaces the old lazy-load
        // "straddler" reconciliation path.
        //
        // Non-bitmap pipelines never write this column and fall through
        // to returning the raw persisted `checkpoint_hi_inclusive`.
        let cp_hi = match self
            .client
            .get_bitmap_bucket_start_cp(pipeline_task)
            .await?
        {
            Some(start) if start > 0 => start - 1,
            _ => {
                if watermark.tx_hi > 0 {
                    warn!(
                        pipeline_task,
                        tx_hi = watermark.tx_hi,
                        checkpoint_hi_inclusive = watermark.checkpoint_hi_inclusive,
                        "bitmap_bucket_start_cp absent; resuming from raw watermark. \
                         If this is a bitmap pipeline, migration from Deploy 1 has not \
                         yet populated the column — operator should verify.",
                    );
                }
                watermark.checkpoint_hi_inclusive
            }
        };
        Ok(Some(InitWatermark {
            checkpoint_hi_inclusive: Some(cp_hi),
            reader_lo: None,
        }))
    }

    async fn accepts_chain_id(
        &mut self,
        _pipeline_task: &str,
        _chain_id: [u8; 32],
    ) -> Result<bool> {
        // TODO: Implement storing chain_id
        Ok(true)
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> Result<Option<CommitterWatermark>> {
        Ok(self
            .client
            .get_pipeline_watermark(pipeline_task)
            .await?
            .map(Into::into))
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> Result<bool> {
        if self.in_sequential_transaction {
            if let Some((prev, _)) = self.pending_watermark.as_ref()
                && prev != pipeline_task
            {
                bail!(
                    "set_committer_watermark called for '{pipeline_task}' \
                    inside a transaction that already staged '{prev}'"
                );
            }
            self.pending_watermark = Some((pipeline_task.to_string(), watermark));
            return Ok(true);
        }

        let pipeline_watermark: Watermark = watermark.into();
        self.client
            .set_pipeline_watermark(pipeline_task, &pipeline_watermark, None)
            .await?;
        Ok(true)
    }
}

#[async_trait]
impl ConcurrentConnection for BigTableConnection<'_> {
    async fn reader_watermark(&mut self, _pipeline: &str) -> Result<Option<ReaderWatermark>> {
        Ok(None)
    }

    async fn pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _delay: Duration,
    ) -> Result<Option<PrunerWatermark>> {
        Ok(None)
    }

    async fn set_reader_watermark(
        &mut self,
        _pipeline: &'static str,
        _reader_lo: u64,
    ) -> Result<bool> {
        Ok(false)
    }

    async fn set_pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _pruner_hi: u64,
    ) -> Result<bool> {
        Ok(false)
    }
}

#[async_trait]
impl SequentialConnection for BigTableConnection<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::BigTableEmulator;
    use crate::testing::INSTANCE_ID;
    use crate::testing::create_tables;
    use crate::testing::require_bigtable_emulator;

    const PIPELINE: &str = "pipeline";
    const EPOCH_HI: u64 = 7;
    const CHECKPOINT_HI: u64 = 200;
    const TX_HI: u64 = 42;
    const TIMESTAMP_MS_HI: u64 = 99;

    /// Spawn a BigTable emulator and return a connected store.
    async fn store_conn() -> (BigTableEmulator, BigTableStore) {
        require_bigtable_emulator();
        let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
            .await
            .unwrap()
            .unwrap();
        create_tables(emulator.host(), INSTANCE_ID).await.unwrap();
        let client = BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.into())
            .await
            .unwrap();
        (emulator, BigTableStore::new(client))
    }

    #[tokio::test]
    async fn test_init_watermark_returns_existing_on_conflict() {
        let (_emulator, store) = store_conn().await;
        let mut conn = store.connect().await.unwrap();

        let watermark = CommitterWatermark {
            epoch_hi_inclusive: EPOCH_HI,
            checkpoint_hi_inclusive: CHECKPOINT_HI,
            tx_hi: TX_HI,
            timestamp_ms_hi_inclusive: TIMESTAMP_MS_HI,
        };
        conn.set_committer_watermark(PIPELINE, watermark)
            .await
            .unwrap();

        // init must surface the existing committer watermark regardless of the input.
        let init = conn
            .init_watermark(PIPELINE, Some(0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, Some(CHECKPOINT_HI));
        // BigTable has no trailing-edge / reader watermark concept.
        assert_eq!(init.reader_lo, None);
    }
}
