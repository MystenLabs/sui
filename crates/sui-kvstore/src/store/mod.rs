// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Framework store facade backed by BigTable.
//!
//! Implements the `Store`, `Connection`, `ConcurrentStore`/`ConcurrentConnection`, and
//! `SequentialStore`/`SequentialConnection` traits over the lower-level
//! [`crate::bigtable::client`] RPC layer. Per-pipeline watermarks are stored
//! in the `watermark_alt` table as:
//! - `w` (BCS v0 `WatermarkV0`) — kept in sync for backward compatibility.
//! - `v` (schema version, currently `1`) — marks the row as using the new schema.
//! - `ehi` / `chi` / `th` / `tmhi` / `rl` / `ph` / `ptm` — one u64 BE cell per
//!   [`WatermarkV1`] field. `chi` (checkpoint) is absent when the committer has not observed
//!   a checkpoint yet.
//! - `b` — bitmap-only bucket-start cell. See `tables::watermarks::col::BUCKET_START_CP`.
//!
//! All three watermark setters (committer, reader, pruner) enforce monotonicity via a
//! BigTable CheckAndMutate CAS on the guarded column. Writes also happen to be idempotent
//! on same-value retry (CAS fails silently).
//!
//! ## Sequential transactions
//!
//! BigTable has no multi-row transaction, so [`SequentialStore::transaction`] runs the
//! closure inline and defers the committer-watermark write until the closure returns
//! successfully. Inside a transaction, `set_committer_watermark` buffers the value on the
//! connection; the transaction impl flushes it to BigTable after the closure succeeds.
//!
//! Bitmap pipelines suppress that deferred synchronous write and let the store-owned
//! committer promote watermarks after row writes are durable. Its OR-based bitmap writes
//! are idempotent under retry because each write contains cumulative row state. On restart,
//! bitmap pipelines resume at the active bucket's replay floor (via the persisted `b`
//! column surfaced through `init_watermark`), and the committer ignores replayed rows for
//! buckets already sealed by the persisted `tx_hi`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use anyhow::bail;
use async_trait::async_trait;
use bytes::Bytes;
use prometheus::Registry;
use scoped_futures::ScopedBoxFuture;
use sui_futures::service::Service;
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

use bitmap_committer::BitmapCommitter;
use bitmap_committer::BitmapCommitterHandle;
use bitmap_committer::BitmapIndexMetrics;
pub(crate) use bitmap_committer::NUM_SHARDS;
pub(crate) use bitmap_committer::shard_for;

use crate::WatermarkV1;
use crate::bigtable::client::BigTableClient;
use crate::handlers::BitmapBatch;
use crate::handlers::BitmapIndexProcessor;
use crate::rate_limiter::CompositeRateLimiter;
use crate::tables::watermarks::col;
use crate::tables::watermarks::decode_v0;
use crate::tables::watermarks::decode_v1;

mod bitmap_committer;

/// Bitmap-pipeline initial watermark — the fields decoded from the watermark
/// row during `init_watermark` that the framework's `InitWatermark` doesn't
/// surface. `watermark` is the full persisted committer watermark (un-clamped:
/// it drives the generation task's bucket identity, which must match
/// `bucket_start_cp`). Processors decide which watermark dimension to gate on
/// via [`BitmapIndexProcessor::is_sealed`].
#[derive(Clone, Copy, Debug, Default)]
pub struct BitmapInitialWatermark {
    pub watermark: CommitterWatermark,
    pub bucket_start_cp: u64,
}

/// A Store implementation backed by BigTable.
#[derive(Clone)]
pub struct BigTableStore {
    client: BigTableClient,
    /// Per-pipeline `init_watermark` results. Populated exactly once per
    /// pipeline — on the first `init_watermark` call — and read-only
    /// thereafter. Shared across every `Clone` / `connect()` of this store
    /// so the framework's `init_watermark` call during pipeline registration
    /// returns the same value as any earlier bootstrap call.
    ///
    /// This is an ordering contract, not a cache: callers of
    /// [`BitmapInitialWatermarks::get`] must ensure `init_watermark` has
    /// already run for the pipeline. A missing entry is a programmer error
    /// surfaced as a task error rather than silently triggering a fresh BigTable read.
    init_results: Arc<Mutex<HashMap<String, PipelineInitResult>>>,
    /// Bitmap committer handles registered by pipeline name. Connections
    /// clone a handle out of this map before awaiting channel sends.
    bitmap_committers: Arc<Mutex<HashMap<String, BitmapCommitterHandle>>>,
}

/// A connection to BigTable for watermark operations and data writes.
///
/// While a [`SequentialStore::transaction`] is in flight, `set_committer_watermark` buffers
/// its write in `pending_watermark` instead of hitting BigTable; the transaction impl
/// flushes it after the transaction closure returns successfully.
pub struct BigTableConnection<'a> {
    client: BigTableClient,
    pending_watermark: Option<(String, CommitterWatermark)>,
    /// `true` while running under `SequentialStore::transaction`.
    in_sequential_transaction: bool,
    /// When `true`, the deferred watermark write inside
    /// [`SequentialStore::transaction`] is skipped even if `pending_watermark`
    /// is staged. Bitmap commits set this because their store-owned committer
    /// promotes watermarks asynchronously after row writes are durable.
    skip_watermark_write: bool,
    /// Shared with the owning [`BigTableStore`]. `init_watermark` writes here on the
    /// first call per `pipeline_task` and reads back on subsequent calls, so the
    /// framework's implicit call during `sequential_pipeline` registration doesn't
    /// duplicate an earlier bootstrap call's BigTable round-trip.
    init_results: Arc<Mutex<HashMap<String, PipelineInitResult>>>,
    /// Shared registry of store-owned bitmap committers.
    bitmap_committers: Arc<Mutex<HashMap<String, BitmapCommitterHandle>>>,
    _marker: std::marker::PhantomData<&'a ()>,
}

/// One-shot builder for background runtime tasks owned by the store facade.
/// `BigTableStore` itself is cloneable framework state; this builder owns the
/// non-clone [`Service`] that must be merged into the top-level indexer once.
pub(crate) struct BigTableStoreRuntimeBuilder {
    store: BigTableStore,
    service: Service,
}

#[derive(Clone)]
pub(crate) struct BitmapInitialWatermarks {
    init_results: Arc<Mutex<HashMap<String, PipelineInitResult>>>,
}

/// Result of a completed `init_watermark` call, recorded so later calls for
/// the same `pipeline_task` return an identical answer without re-reading
/// the watermark row. Written exactly once per pipeline (on the first
/// `init_watermark` call); later calls read-only.
#[derive(Clone)]
struct PipelineInitResult {
    init: Option<InitWatermark>,
    bitmap: BitmapInitialWatermark,
}

impl BigTableStore {
    pub fn new(client: BigTableClient) -> Self {
        Self {
            client,
            init_results: Arc::new(Mutex::new(HashMap::new())),
            bitmap_committers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Cloned handle to the underlying client. Cheap: `BigTableClient` is
    /// a thin `Clone` wrapper over shared gRPC channels.
    pub fn client(&self) -> BigTableClient {
        self.client.clone()
    }

    pub(crate) fn bitmap_initial_watermarks(&self) -> BitmapInitialWatermarks {
        BitmapInitialWatermarks {
            init_results: self.init_results.clone(),
        }
    }

    pub(crate) fn runtime_builder(&self) -> BigTableStoreRuntimeBuilder {
        BigTableStoreRuntimeBuilder {
            store: self.clone(),
            service: Service::new(),
        }
    }
}

impl BigTableConnection<'_> {
    /// Returns a mutable reference to the underlying BigTable client.
    pub fn client(&mut self) -> &mut BigTableClient {
        &mut self.client
    }

    /// Enqueue a bitmap batch into the store-owned committer and suppress the
    /// framework's synchronous watermark write for this transaction.
    pub(crate) async fn commit_bitmap_batch<P>(&mut self, batch: &BitmapBatch) -> Result<usize>
    where
        P: BitmapIndexProcessor + Send + Sync + 'static,
    {
        let Some((pipeline, watermark)) = self.pending_watermark.as_ref() else {
            bail!("set_committer_watermark must be called before bitmap handler commit");
        };
        if pipeline != P::NAME {
            bail!(
                "bitmap handler `{}` saw staged watermark for `{pipeline}`",
                P::NAME
            );
        }
        let watermark = *watermark;

        // Always suppress the framework's deferred-watermark write. Even
        // empty-batch commits must flow through the async bitmap pipeline so
        // their watermarks are promoted in order after any prior in-flight
        // row writes become durable.
        self.skip_watermark_write = true;

        let committer = self
            .bitmap_committers
            .lock()
            .unwrap()
            .get(P::NAME)
            .cloned()
            .unwrap_or_else(|| panic!("bitmap committer for `{}` is not registered", P::NAME));

        if committer
            .commit(batch.clone_shards(), watermark)
            .await
            .is_err()
        {
            bail!("{}: bitmap merge loop exited; cannot continue", P::NAME);
        }

        // Bitmap row writes happen asynchronously after `commit()` returns, so
        // this is a lagging drain of rows accepted by BigTable since the last
        // framework commit for this pipeline.
        Ok(committer.take_rows_written())
    }

    fn record_init_result(
        &self,
        pipeline_task: &str,
        init: Option<InitWatermark>,
        watermark: CommitterWatermark,
        bucket_start_cp: u64,
    ) {
        self.init_results.lock().unwrap().insert(
            pipeline_task.to_string(),
            PipelineInitResult {
                init,
                bitmap: BitmapInitialWatermark {
                    watermark,
                    bucket_start_cp,
                },
            },
        );
    }

    /// Read a watermark for read-side methods. Enforces the "hide if `checkpoint < reader_lo`
    /// or `checkpoint == None`" rule and returns the unwrapped checkpoint for callers that
    /// need it.
    async fn get_watermark_for_read(
        &mut self,
        pipeline: &str,
    ) -> Result<Option<(WatermarkV1, u64)>> {
        let row = self.client.get_pipeline_watermark_rows(pipeline).await?;
        let Some(watermark) = decode_v1(&row)? else {
            return Ok(None);
        };
        let Some(checkpoint_hi_inclusive) = watermark
            .checkpoint_hi_inclusive
            .filter(|&cp| cp >= watermark.reader_lo)
        else {
            return Ok(None);
        };
        Ok(Some((watermark, checkpoint_hi_inclusive)))
    }
}

impl BigTableStoreRuntimeBuilder {
    pub(crate) fn with_bitmap_committer<P>(
        mut self,
        write_chunk_size: usize,
        write_concurrency: usize,
        rate_limiter: Arc<CompositeRateLimiter>,
        registry: Option<&Registry>,
    ) -> Self
    where
        P: BitmapIndexProcessor + Send + Sync + 'static,
    {
        let metrics = match registry {
            Some(reg) => BitmapIndexMetrics::new(P::NAME, reg),
            None => BitmapIndexMetrics::noop(),
        };

        let (handle, service) = BitmapCommitter {
            pipeline: P::NAME,
            table: P::TABLE,
            column: P::COLUMN,
            is_sealed: P::is_sealed,
            initial_watermarks: self.store.bitmap_initial_watermarks(),
            client: self.store.client.clone(),
            rate_limiter,
            write_chunk_size,
            write_concurrency,
            metrics,
        }
        .spawn();

        assert!(
            self.store
                .bitmap_committers
                .lock()
                .unwrap()
                .insert(P::NAME.to_string(), handle)
                .is_none(),
            "bitmap committer for pipeline `{}` registered more than once",
            P::NAME,
        );

        self.service = self.service.merge(service);
        self
    }

    pub(crate) fn into_service(self) -> Service {
        self.service
    }
}

impl BitmapInitialWatermarks {
    /// Bitmap fields recorded during the first `init_watermark` call for
    /// `pipeline_task`. Returns an error if `init_watermark` has not yet been
    /// invoked for this pipeline — framework must drive `init_watermark` first.
    pub(crate) fn get(&self, pipeline_task: &str) -> Result<BitmapInitialWatermark> {
        self.init_results
            .lock()
            .unwrap()
            .get(pipeline_task)
            .map(|result| result.bitmap)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "bitmap initial watermark requested for `{pipeline_task}` before \
                     init_watermark; the indexer bootstrap must run init_watermark for \
                     every bitmap pipeline before its first commit",
                )
            })
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
            init_results: self.init_results.clone(),
            bitmap_committers: self.bitmap_committers.clone(),
            _marker: std::marker::PhantomData,
        })
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
        // Closure returned `Ok` — now persist the staged watermark unless a
        // store-owned side effect suppressed it. The flush goes through the
        // CAS-guarded `set_committer_watermark` path (via the client helper),
        // so concurrent writers cannot cause regressions.
        if let Some((pipeline, watermark)) = conn.pending_watermark.take()
            && !conn.skip_watermark_write
        {
            conn.client
                .set_committer_watermark_cells(&pipeline, &watermark, None)
                .await?;
        }
        Ok(result)
    }
}

#[async_trait]
impl Connection for BigTableConnection<'_> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        checkpoint_hi_inclusive: Option<u64>,
    ) -> Result<Option<InitWatermark>> {
        // Re-entry for the same `pipeline_task` returns the previously computed
        // answer without a BigTable round-trip. Also lets callers with a legitimate
        // need for bitmap fields pre-seed via an explicit bootstrap call.
        if let Some(existing) = self.init_results.lock().unwrap().get(pipeline_task) {
            return Ok(existing.init);
        }

        // This initial read is to determine if we need to migrate from v0 to v1 and to pick
        // up the bitmap `b` cell (if any) so bitmap pipelines resume at their bucket start.
        let row = self
            .client
            .get_pipeline_watermark_rows(pipeline_task)
            .await?;
        let existing_v1 = decode_v1(&row)?;
        let existing_v0 = decode_v0(&row)?;

        // Case 1: row already in the v1 format → return its values, no write.
        if let Some(wm) = existing_v1 {
            let bucket_start_cp = wm.bucket_start_cp;
            let init = InitWatermark {
                checkpoint_hi_inclusive: clamp_to_bucket_start(
                    wm.checkpoint_hi_inclusive,
                    bucket_start_cp,
                ),
                reader_lo: Some(wm.reader_lo),
            };
            let watermark = CommitterWatermark {
                epoch_hi_inclusive: wm.epoch_hi_inclusive,
                checkpoint_hi_inclusive: wm.checkpoint_hi_inclusive.unwrap_or(0),
                tx_hi: wm.tx_hi,
                timestamp_ms_hi_inclusive: wm.timestamp_ms_hi_inclusive,
            };
            self.record_init_result(
                pipeline_task,
                Some(init),
                watermark,
                bucket_start_cp.unwrap_or(0),
            );
            return Ok(Some(init));
        }

        let initial = if let Some(v0) = existing_v0 {
            // Case 2: v0-only row → bootstrap a v1 watermark from the v0 committer fields.
            let reader_lo = v0.checkpoint_hi_inclusive + 1;
            WatermarkV1 {
                epoch_hi_inclusive: v0.epoch_hi_inclusive,
                checkpoint_hi_inclusive: Some(v0.checkpoint_hi_inclusive),
                tx_hi: v0.tx_hi,
                timestamp_ms_hi_inclusive: v0.timestamp_ms_hi_inclusive,
                reader_lo,
                pruner_hi: reader_lo,
                pruner_timestamp_ms: 0,
                bucket_start_cp: None,
            }
        } else {
            // Case 3: nothing exists → write a fresh row from the framework's input.
            let reader_lo = checkpoint_hi_inclusive.map_or(0, |cp| cp + 1);
            WatermarkV1 {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
                reader_lo,
                pruner_hi: reader_lo,
                pruner_timestamp_ms: 0,
                bucket_start_cp: None,
            }
        };

        let write_happened = self
            .client
            .create_pipeline_watermark_if_absent(pipeline_task, &initial)
            .await?;

        let final_wm = if write_happened {
            initial
        } else {
            let row = self
                .client
                .get_pipeline_watermark_rows(pipeline_task)
                .await?;
            decode_v1(&row)?.ok_or_else(|| {
                anyhow::anyhow!(
                    "watermark for pipeline {} missing after creation",
                    pipeline_task
                )
            })?
        };

        let init = InitWatermark {
            checkpoint_hi_inclusive: clamp_to_bucket_start(
                final_wm.checkpoint_hi_inclusive,
                final_wm.bucket_start_cp,
            ),
            reader_lo: Some(final_wm.reader_lo),
        };
        let watermark = CommitterWatermark {
            epoch_hi_inclusive: final_wm.epoch_hi_inclusive,
            checkpoint_hi_inclusive: final_wm.checkpoint_hi_inclusive.unwrap_or(0),
            tx_hi: final_wm.tx_hi,
            timestamp_ms_hi_inclusive: final_wm.timestamp_ms_hi_inclusive,
        };
        self.record_init_result(
            pipeline_task,
            Some(init),
            watermark,
            final_wm.bucket_start_cp.unwrap_or(0),
        );
        Ok(Some(init))
    }

    async fn accepts_chain_id(&mut self, pipeline_task: &str, chain_id: [u8; 32]) -> Result<bool> {
        self.client.accepts_chain_id(pipeline_task, chain_id).await
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> Result<Option<CommitterWatermark>> {
        Ok(self.get_watermark_for_read(pipeline_task).await?.map(
            |(wm, checkpoint_hi_inclusive)| CommitterWatermark {
                epoch_hi_inclusive: wm.epoch_hi_inclusive,
                checkpoint_hi_inclusive,
                tx_hi: wm.tx_hi,
                timestamp_ms_hi_inclusive: wm.timestamp_ms_hi_inclusive,
            },
        ))
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

        self.client
            .set_committer_watermark_cells(pipeline_task, &watermark, None)
            .await
    }
}

#[async_trait]
impl ConcurrentConnection for BigTableConnection<'_> {
    async fn reader_watermark(&mut self, pipeline: &str) -> Result<Option<ReaderWatermark>> {
        Ok(self
            .get_watermark_for_read(pipeline)
            .await?
            .map(|(wm, checkpoint_hi_inclusive)| ReaderWatermark {
                checkpoint_hi_inclusive,
                reader_lo: wm.reader_lo,
            }))
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> Result<Option<PrunerWatermark>> {
        let Some((watermark, _)) = self.get_watermark_for_read(pipeline).await? else {
            return Ok(None);
        };
        // Compute max(0, (pruner_timestamp + delay) - now). Use u128 to avoid overflow when
        // summing the two operands, and saturating_sub so we never underflow when the wait
        // period has already elapsed. saturating_sub is safe because callers treat anything
        // < 1 the same.
        let pruner_ready_ms = (watermark.pruner_timestamp_ms as u128) + delay.as_millis();
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let wait_for_ms = i64::try_from(pruner_ready_ms.saturating_sub(now_ms))?;
        Ok(Some(PrunerWatermark {
            wait_for_ms,
            reader_lo: watermark.reader_lo,
            pruner_hi: watermark.pruner_hi,
        }))
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> Result<bool> {
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let cells = vec![
            (col::READER_LO, u64_be(reader_lo)),
            (col::PRUNER_TIMESTAMP_MS, u64_be(now_ms)),
        ];
        self.client
            .cas_write_pipeline_watermark_cells(pipeline, col::READER_LO, reader_lo, cells)
            .await
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> Result<bool> {
        let cells = vec![(col::PRUNER_HI, u64_be(pruner_hi))];
        self.client
            .cas_write_pipeline_watermark_cells(pipeline, col::PRUNER_HI, pruner_hi, cells)
            .await
    }
}

#[async_trait]
impl SequentialConnection for BigTableConnection<'_> {}

fn u64_be(v: u64) -> Bytes {
    Bytes::copy_from_slice(&v.to_be_bytes())
}

/// If a bitmap `bucket_start_cp` is persisted, clamp the returned
/// `checkpoint_hi_inclusive` back so the framework resumes at the start of
/// the currently-active bucket. Non-bitmap pipelines never write `b`, so
/// `bucket_start_cp` is `None` and the raw checkpoint passes through.
fn clamp_to_bucket_start(
    checkpoint_hi_inclusive: Option<u64>,
    bucket_start_cp: Option<u64>,
) -> Option<u64> {
    match bucket_start_cp {
        Some(0) => None,
        Some(start) if start > 0 => Some(start - 1),
        _ => checkpoint_hi_inclusive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::BigTableEmulator;
    use crate::testing::INSTANCE_ID;
    use crate::testing::create_tables;
    use crate::testing::require_bigtable_emulator;

    const PIPELINE: &str = "pipeline";
    const BITMAP_TX_HI: u64 = 500;

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

    #[test]
    fn test_clamp_bucket_start_zero_replays_from_genesis() {
        assert_eq!(clamp_to_bucket_start(Some(42), Some(0)), None);
    }

    #[tokio::test]
    async fn test_init_watermark_clamps_to_bucket_start() {
        let (_emulator, store) = store_conn().await;
        let mut conn = store.connect().await.unwrap();
        conn.client()
            .create_pipeline_watermark_if_absent(
                PIPELINE,
                &WatermarkV1 {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: Some(42),
                    tx_hi: BITMAP_TX_HI,
                    timestamp_ms_hi_inclusive: 1000,
                    reader_lo: 0,
                    pruner_hi: 0,
                    pruner_timestamp_ms: 0,
                    bucket_start_cp: Some(10),
                },
            )
            .await
            .unwrap();

        let init = conn.init_watermark(PIPELINE, None).await.unwrap().unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, Some(9));
    }

    #[tokio::test]
    async fn test_init_watermark_falls_back_when_bucket_start_absent() {
        let (_emulator, store) = store_conn().await;
        let mut conn = store.connect().await.unwrap();
        let watermark = CommitterWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 42,
            tx_hi: BITMAP_TX_HI,
            timestamp_ms_hi_inclusive: 1000,
        };
        conn.client()
            .set_committer_watermark_cells(PIPELINE, &watermark, None)
            .await
            .unwrap();

        let init = conn.init_watermark(PIPELINE, None).await.unwrap().unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, Some(42));
    }
}
