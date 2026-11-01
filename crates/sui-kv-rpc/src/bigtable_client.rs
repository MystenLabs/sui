// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Request-scoped BigTable facade.
//!
//! Downstream read APIs on this wrapper acquire a permit from a single
//! `Arc<Semaphore>` before dispatching to the underlying
//! `sui_kvstore::BigTableClient`.
//! For streaming methods, the permit is bundled into the returned stream's
//! state and released only when the stream is dropped. Pipeline helpers drain
//! those streams inside one chunk future so live permit-holding streams do not
//! cross stage boundaries. For non-streaming methods, the permit is held for
//! the duration of the call.
//!
//! Bitmap scans are intentionally not gated by this downstream semaphore:
//! their fanout is bounded by `max_bitmap_filter_literals`, and filtered list
//! requests must still be able to run bitmap discovery with a low downstream
//! request budget.
//!
//! The wrapper is constructed per request (`BigTableClient::new(client, capacity, ...)`)
//! and threaded down through the pipeline.
//!
//! Each successful acquire wait is labeled with `stage` so the wait histogram
//! can be sliced by call-site type. Per-request peak permits and successful
//! acquisition count are recorded once on `BigTableClientContext` drop.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use bytes::Bytes;
use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use prometheus::HistogramVec;
use prometheus::IntCounterVec;
use prometheus::Registry;
use prometheus::register_histogram_vec_with_registry;
use prometheus::register_int_counter_vec_with_registry;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::ScanStop;
use sui_inverted_index::SkipPolicy;
use sui_inverted_index::WatermarkPacing;
use sui_inverted_index::Watermarked;
use sui_inverted_index::eval_bitmap_query_stream;
use sui_kvstore::BigTableBitmapSource;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::CheckpointData;
use sui_kvstore::RowFilter;
use sui_kvstore::TransactionData;
use sui_kvstore::TxSeqDigestData;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::ledger_point::RowPlacement;
use sui_rpc_api::ledger_history::query_options::IntraTxCoordinate;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;

use crate::pipeline::PlacedFrontier;

/// Stage label values for the permit timing metrics. `&'static str` so they
/// satisfy `with_label_values` zero-allocation.
pub(crate) mod stage {
    pub const TRANSACTIONS: &str = "transactions";
    pub const OBJECTS: &str = "objects";
    pub const TX_SEQ_DIGEST: &str = "tx_seq_digest";
    pub const TX_CP_RESOLVE: &str = "tx_cp_resolve";
    pub const CHECKPOINTS: &str = "checkpoints";
}

/// Metrics for the request-scoped `BigTableClient` limiter and the reads it
/// gates. Cardinality is bounded: per method, 5 stages for `permit_wait_ms`,
/// `permit_hold_ms`, and `reads_started`, 5 stages × 3 outcomes for
/// `reads_finished`, and one series each for the request-end `permits_peak`
/// and `ops_total` histograms.
#[derive(Clone)]
pub(crate) struct Metrics {
    /// Time spent waiting for one limiter permit, recorded once per successful
    /// `acquire_owned()` call.
    pub permit_wait_ms: HistogramVec,
    /// Time one limiter permit is held from acquisition through release.
    pub permit_hold_ms: HistogramVec,
    /// Peak in-use permit count observed during a single request, recorded
    /// once when the request's `BigTableClient` is dropped.
    pub permits_peak: HistogramVec,
    /// Total limiter acquisitions issued by a single request, recorded once
    /// when the request's `BigTableClient` is dropped.
    pub ops_total: HistogramVec,
    /// Gated reads started, counted when the read's permit is acquired, just
    /// before it is sent.
    pub reads_started: IntCounterVec,
    /// Gated reads that ended, by [`ReadOutcome`], counted when the read's
    /// permit is released.
    pub reads_finished: IntCounterVec,
}

impl Metrics {
    pub(crate) fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            permit_wait_ms: register_histogram_vec_with_registry!(
                "kv_rpc_bigtable_permit_wait_ms",
                "Wait time for a request-scoped BigTable concurrency-limiter permit, per acquire.",
                &["method", "stage"],
                prometheus::exponential_buckets(0.1, 2.0, 16).unwrap(),
                registry,
            )
            .unwrap(),
            permit_hold_ms: register_histogram_vec_with_registry!(
                "kv_rpc_bigtable_permit_hold_ms",
                "Time a request-scoped BigTable limiter permit is held, from successful acquire to release (the gated stream's full drain or drop).",
                &["method", "stage"],
                prometheus::exponential_buckets(0.5, 2.0, 16).unwrap(),
                registry,
            )
            .unwrap(),
            permits_peak: register_histogram_vec_with_registry!(
                "kv_rpc_bigtable_permits_peak",
                "Peak in-use BigTable limiter permits observed during a single list request.",
                &["method"],
                vec![
                    1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 50.0, 65.0, 80.0,
                    100.0, 130.0, 166.0, 200.0,
                ],
                registry,
            )
            .unwrap(),
            ops_total: register_histogram_vec_with_registry!(
                "kv_rpc_bigtable_ops_total",
                "Total BigTable limiter acquisitions issued by a single list request.",
                &["method"],
                prometheus::exponential_buckets(1.0, 2.0, 14).unwrap(),
                registry,
            )
            .unwrap(),
            reads_started: register_int_counter_vec_with_registry!(
                "kv_rpc_bigtable_reads_started_total",
                "BigTable reads started under the request-scoped limiter, counted when the read acquires its permit, just before it is sent. Each read is normally one ReadRows call.",
                &["method", "stage"],
                registry,
            )
            .unwrap(),
            reads_finished: register_int_counter_vec_with_registry!(
                "kv_rpc_bigtable_reads_finished_total",
                "BigTable reads under the request-scoped limiter that ended, by outcome: completed (the response was read to its end), failed (BigTable or decoding returned an error), or cancelled (dropped before either; a cancelled read that was already sent resets its HTTP/2 stream). Started minus finished is the number in flight.",
                &["method", "stage", "outcome"],
                registry,
            )
            .unwrap(),
        })
    }

    #[cfg(test)]
    pub(crate) fn for_testing() -> (Arc<Self>, Registry) {
        let registry = Registry::new();
        let metrics = Self::new(&registry);
        (metrics, registry)
    }
}

#[derive(Clone)]
pub(crate) struct BigTableClient {
    inner: sui_kvstore::BigTableClient,
    limiter: Arc<Semaphore>,
    context: Arc<BigTableClientContext>,
}

/// Per-request shared state. Cloned along with `BigTableClient` (each clone is
/// just an `Arc` bump). When the last clone is dropped, `Drop` records the
/// per-request peak and ops-total histograms.
pub(crate) struct BigTableClientContext {
    metrics: Arc<Metrics>,
    method: &'static str,
    active_permits: AtomicUsize,
    peak_used: AtomicUsize,
    ops_count: AtomicUsize,
}

impl Drop for BigTableClientContext {
    fn drop(&mut self) {
        let peak = *self.peak_used.get_mut() as f64;
        let ops = *self.ops_count.get_mut() as f64;
        self.metrics
            .permits_peak
            .with_label_values(&[self.method])
            .observe(peak);
        self.metrics
            .ops_total
            .with_label_values(&[self.method])
            .observe(ops);
    }
}

impl BigTableClient {
    pub(crate) fn new(
        inner: sui_kvstore::BigTableClient,
        capacity: usize,
        metrics: Arc<Metrics>,
        method: &'static str,
    ) -> Self {
        Self {
            inner,
            limiter: Arc::new(Semaphore::new(capacity)),
            context: Arc::new(BigTableClientContext {
                metrics,
                method,
                active_permits: AtomicUsize::new(0),
                peak_used: AtomicUsize::new(0),
                ops_count: AtomicUsize::new(0),
            }),
        }
    }

    /// Pure builder, no RPC. Passthrough so callers don't need to reach for
    /// `sui_kvstore::BigTableClient` (which would expose the unlimited surface).
    pub(crate) fn column_filter(columns: &[&str]) -> RowFilter {
        sui_kvstore::BigTableClient::column_filter(columns)
    }

    pub(crate) async fn multi_get_stream(
        &self,
        table_name: &str,
        keys: Vec<Vec<u8>>,
        filter: Option<RowFilter>,
        stage: &'static str,
    ) -> Result<BoxStream<'static, Result<(Bytes, Vec<(Bytes, Bytes)>), anyhow::Error>>, RpcError>
    {
        let mut inner = self.inner.clone();
        self.open_gated_stream(stage, inner.multi_get_stream(table_name, keys, filter))
            .await
    }

    pub(crate) async fn get_transactions_stream(
        &self,
        digests: Vec<TransactionDigest>,
        column_filter: Option<RowFilter>,
    ) -> Result<
        BoxStream<'static, Result<(TransactionDigest, TransactionData), anyhow::Error>>,
        RpcError,
    > {
        let mut inner = self.inner.clone();
        self.open_gated_stream(
            stage::TRANSACTIONS,
            inner.get_transactions_stream(digests, column_filter),
        )
        .await
    }

    pub(crate) async fn get_objects_stream(
        &self,
        object_keys: Vec<ObjectKey>,
    ) -> Result<BoxStream<'static, Result<Object, anyhow::Error>>, RpcError> {
        let mut inner = self.inner.clone();
        self.open_gated_stream(stage::OBJECTS, inner.get_objects_stream(object_keys))
            .await
    }

    pub(crate) async fn resolve_tx_digests_stream(
        &self,
        tx_sequence_numbers: Vec<u64>,
    ) -> Result<BoxStream<'static, Result<TxSeqDigestData, anyhow::Error>>, RpcError> {
        let mut inner = self.inner.clone();
        self.open_gated_stream(
            stage::TX_SEQ_DIGEST,
            inner.resolve_tx_digests_stream(tx_sequence_numbers),
        )
        .await
    }

    pub(crate) async fn scan_tx_seq_digests_stream(
        &self,
        range: Range<u64>,
        direction: ScanDirection,
        rows_limit: usize,
    ) -> Result<BoxStream<'static, Result<TxSeqDigestData, anyhow::Error>>, RpcError> {
        self.inner
            .clone()
            .scan_tx_seq_digests_stream(range, direction, rows_limit)
            .await
            .map(|stream| stream.boxed())
            .map_err(RpcError::from)
    }

    pub(crate) async fn scan_checkpoints_stream(
        &self,
        range: Range<u64>,
        direction: ScanDirection,
        rows_limit: usize,
        filter: Option<RowFilter>,
    ) -> Result<BoxStream<'static, Result<(u64, CheckpointData), anyhow::Error>>, RpcError> {
        self.inner
            .clone()
            .scan_checkpoints_stream(range, direction, rows_limit, filter)
            .await
            .map(|stream| stream.boxed())
            .map_err(RpcError::from)
    }

    pub(crate) async fn checkpoint_to_tx_range(
        &self,
        checkpoint_range: Range<u64>,
    ) -> Result<Range<u64>, RpcError> {
        let mut inner = self.inner.clone();
        self.call_gated(
            stage::CHECKPOINTS,
            inner.checkpoint_to_tx_range(checkpoint_range),
        )
        .await
    }

    /// Read the tx_sequence_number of each of `checkpoints`' first transaction in one call, and
    /// return a lookup over them, as `CheckpointPositions` windows take it. Looking up a
    /// checkpoint outside `checkpoints` is `Internal`.
    pub(crate) async fn checkpoint_tx_boundaries(
        &self,
        checkpoints: Vec<u64>,
    ) -> Result<impl Fn(u64) -> Result<u64, RpcError> + use<>, RpcError> {
        let mut inner = self.inner.clone();
        let boundaries: HashMap<u64, u64> = self
            .call_gated(
                stage::CHECKPOINTS,
                inner.checkpoint_tx_boundaries(&checkpoints),
            )
            .await?;
        Ok(move |checkpoint| {
            boundaries.get(&checkpoint).copied().ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("tx boundary of checkpoint {checkpoint} was not read"),
                )
            })
        })
    }

    /// Read the `tx_seq_digest` row of one transaction under a `tx_cp_resolve`
    /// permit, `None` when it has none.
    pub(crate) async fn tx_seq_digest_row(
        &self,
        tx_seq: u64,
    ) -> Result<Option<TxSeqDigestData>, RpcError> {
        let mut inner = self.inner.clone();
        let rows = self
            .call_gated(stage::TX_CP_RESOLVE, inner.resolve_tx_digests(&[tx_seq]))
            .await?;
        Ok(rows.into_iter().flatten().next())
    }

    /// Stream the checkpoint of each of `tx_sequence_numbers`, reading only the
    /// checkpoint-number column of `tx_seq_digest`. The permit is held for the
    /// stream's lifetime (see `gate_stream`); rows arrive in BigTable order.
    pub(crate) async fn resolve_tx_checkpoints_stream(
        &self,
        tx_sequence_numbers: Vec<u64>,
    ) -> Result<BoxStream<'static, Result<(u64, CheckpointSequenceNumber), anyhow::Error>>, RpcError>
    {
        let mut inner = self.inner.clone();
        self.open_gated_stream(
            stage::TX_CP_RESOLVE,
            inner.resolve_tx_checkpoints_stream(tx_sequence_numbers),
        )
        .await
    }

    /// Eval a `BitmapQuery`. Bitmap scans stay outside the downstream request
    /// semaphore; their concurrency is bounded by `max_bitmap_filter_literals`
    /// during filter validation. `pacing` decides which scan watermarks reach
    /// the stream (see [`crate::operation::QueryContext::watermark_pacing`]).
    pub(crate) fn eval_bitmap_query_stream<F>(
        &self,
        query: BitmapQuery,
        range: Range<u64>,
        spec: BitmapIndexSpec,
        direction: ScanDirection,
        budget: u64,
        policy: SkipPolicy,
        pacing: WatermarkPacing,
        on_metrics: F,
    ) -> BoxStream<'static, Result<Watermarked<u64>, ScanStop>>
    where
        F: FnOnce(sui_inverted_index::BitmapScanMetrics) + Send + 'static,
    {
        let source = BigTableBitmapSource::new(self.inner.clone(), spec);
        eval_bitmap_query_stream(
            source,
            query,
            range,
            spec.bucket_size,
            direction,
            budget,
            policy,
            pacing,
            on_metrics,
        )
    }

    async fn acquire(&self, stage: &'static str) -> Result<LimitedPermit, RpcError> {
        acquire_limited(self.limiter.clone(), self.context.clone(), stage)
            .await
            .map_err(|_| RpcError::new(tonic::Code::Internal, "request concurrency limiter closed"))
    }

    /// Open a streaming read under a `stage` permit. The permit stays with the
    /// returned stream until it is dropped, and records how the read ended.
    async fn open_gated_stream<T, S>(
        &self,
        stage: &'static str,
        open: impl Future<Output = anyhow::Result<S>>,
    ) -> Result<BoxStream<'static, Result<T, anyhow::Error>>, RpcError>
    where
        T: Send + 'static,
        S: Stream<Item = anyhow::Result<T>> + Send + 'static,
    {
        let mut permit = self.acquire(stage).await?;
        match open.await {
            Ok(inner) => Ok(gate_stream(permit, inner.boxed())),
            Err(error) => {
                permit.finish(ReadOutcome::Failed);
                Err(RpcError::from(error))
            }
        }
    }

    /// Run a read that returns its whole result under a `stage` permit.
    async fn call_gated<T>(
        &self,
        stage: &'static str,
        call: impl Future<Output = anyhow::Result<T>>,
    ) -> Result<T, RpcError> {
        let mut permit = self.acquire(stage).await?;
        let result = call.await;
        permit.finish(if result.is_ok() {
            ReadOutcome::Completed
        } else {
            ReadOutcome::Failed
        });
        result.map_err(RpcError::from)
    }

    /// Place a scan frontier by the one `tx_seq_digest` row `placement` names
    /// (see [`RowPlacement::transaction_boundary`] and
    /// [`RowPlacement::event_boundary`]): `None`, without a read, for the
    /// ascending genesis frontier, which covers no checkpoint and lies at
    /// [`LedgerPoint::GENESIS`]. A missing row is `Internal`.
    ///
    /// [`LedgerPoint::GENESIS`]: sui_rpc_api::ledger_history::ledger_point::LedgerPoint::GENESIS
    pub(crate) async fn place_frontier(
        &self,
        placement: Option<RowPlacement>,
    ) -> Result<Option<PlacedFrontier>, RpcError> {
        let Some(placement) = placement else {
            return Ok(None);
        };
        let row = self
            .tx_seq_digest_row(placement.tx_seq)
            .await?
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "missing checkpoint mapping for transaction sequence number {}",
                        placement.tx_seq
                    ),
                )
            })?;
        Ok(Some(PlacedFrontier {
            checkpoint: row.checkpoint_number,
            point: placement.point(row.checkpoint_number, row.tx_offset as u64),
        }))
    }

    /// Build a resolver closure for tx-bitmap watermarks (identity
    /// decode). Hand directly to [`crate::pipeline::resolve_scan_watermarks`].
    pub(crate) fn tx_wm_resolver(
        &self,
        direction: ScanDirection,
    ) -> impl Fn(u64) -> WmResolverFut + Send + 'static {
        let client = self.clone();
        move |position| {
            let client = client.clone();
            Box::pin(async move {
                client
                    .place_frontier(RowPlacement::transaction_boundary(position, direction))
                    .await
                    .map_err(|e| ScanStop::Fault(anyhow::Error::new(e)))
            })
        }
    }

    /// Build a resolver closure for event-bitmap watermarks. Hand directly to
    /// [`crate::pipeline::resolve_scan_watermarks`].
    pub(crate) fn event_wm_resolver(
        &self,
        direction: ScanDirection,
    ) -> impl Fn(IntraTxCoordinate) -> WmResolverFut + Send + 'static {
        let client = self.clone();
        move |position| {
            let client = client.clone();
            Box::pin(async move {
                client
                    .place_frontier(RowPlacement::event_boundary(position, direction))
                    .await
                    .map_err(|e| ScanStop::Fault(anyhow::Error::new(e)))
            })
        }
    }
}

/// Future returned by resolver closures from
/// [`BigTableClient::tx_wm_resolver`] and [`BigTableClient::event_wm_resolver`].
pub(crate) type WmResolverFut =
    std::pin::Pin<Box<dyn Future<Output = Result<Option<PlacedFrontier>, ScanStop>> + Send>>;

/// How a gated read ended, recorded once when its permit is released.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadOutcome {
    /// The response was read to its end.
    Completed,
    /// BigTable or decoding returned an error.
    Failed,
    /// Dropped before completing or failing. If the read was already sent,
    /// dropping it resets its HTTP/2 stream.
    Cancelled,
}

impl ReadOutcome {
    fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

struct LimitedPermit {
    _permit: OwnedSemaphorePermit,
    context: Arc<BigTableClientContext>,
    stage: &'static str,
    acquired_at: Instant,
    /// `Cancelled` until the read records how it ended.
    outcome: ReadOutcome,
}

impl LimitedPermit {
    fn finish(&mut self, outcome: ReadOutcome) {
        self.outcome = outcome;
    }
}

impl Drop for LimitedPermit {
    fn drop(&mut self) {
        let hold_ms = self.acquired_at.elapsed().as_secs_f64() * 1000.0;
        let metrics = &self.context.metrics;
        let method = self.context.method;
        metrics
            .permit_hold_ms
            .with_label_values(&[method, self.stage])
            .observe(hold_ms);
        metrics
            .reads_finished
            .with_label_values(&[method, self.stage, self.outcome.label()])
            .inc();
        record_release(&self.context);
    }
}

async fn acquire_limited(
    limiter: Arc<Semaphore>,
    context: Arc<BigTableClientContext>,
    stage: &'static str,
) -> Result<LimitedPermit, tokio::sync::AcquireError> {
    let start = Instant::now();
    let permit = limiter.acquire_owned().await?;
    let acquired_at = Instant::now();
    record_acquired(&context, stage, acquired_at.duration_since(start));
    Ok(LimitedPermit {
        _permit: permit,
        context,
        stage,
        acquired_at,
        outcome: ReadOutcome::Cancelled,
    })
}

fn record_acquired(context: &BigTableClientContext, stage: &'static str, wait: Duration) {
    let wait_ms = wait.as_secs_f64() * 1000.0;
    context
        .metrics
        .permit_wait_ms
        .with_label_values(&[context.method, stage])
        .observe(wait_ms);
    context
        .metrics
        .reads_started
        .with_label_values(&[context.method, stage])
        .inc();
    context.ops_count.fetch_add(1, Ordering::Relaxed);
    let active = context.active_permits.fetch_add(1, Ordering::AcqRel) + 1;
    context.peak_used.fetch_max(active, Ordering::Relaxed);
}

fn record_release(context: &BigTableClientContext) {
    let previous = context
        .active_permits
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
            active.checked_sub(1)
        })
        .unwrap_or(0);
    debug_assert!(previous > 0, "limited client permit release underflow");
}

/// Wrap a stream so that `permit` is held until the stream is dropped (either
/// by full drain, early termination by the consumer, or cancellation
/// propagating through `Drop`). Items and errors pass through unchanged; the
/// permit records the read as completed when the inner stream ends, failed
/// when it yields an error, and cancelled if dropped before either.
fn gate_stream<T>(
    mut permit: LimitedPermit,
    inner: BoxStream<'static, Result<T, anyhow::Error>>,
) -> BoxStream<'static, Result<T, anyhow::Error>>
where
    T: Send + 'static,
{
    async_stream::try_stream! {
        futures::pin_mut!(inner);
        while let Some(row) = inner.next().await {
            if row.is_err() {
                permit.finish(ReadOutcome::Failed);
            }
            yield row?;
        }
        permit.finish(ReadOutcome::Completed);
    }
    .boxed()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use anyhow::anyhow;
    use futures::TryStreamExt;
    use futures::stream;
    use prometheus::proto::MetricFamily;

    use super::*;
    use sui_rpc_api::ledger_history::ledger_point::LedgerPoint;

    /// A `tx_cp_resolve` permit, plus the registry its metrics are recorded in.
    async fn test_permit(limiter: &Arc<Semaphore>) -> (LimitedPermit, Registry) {
        let (metrics, registry) = Metrics::for_testing();
        let context = test_context(metrics, "list_events");
        let permit = acquire_limited(limiter.clone(), context, stage::TX_CP_RESOLVE)
            .await
            .unwrap();
        (permit, registry)
    }

    /// Value of `kv_rpc_bigtable_reads_finished_total` for one outcome.
    fn reads_finished(registry: &Registry, outcome: &str) -> u64 {
        let families = registry.gather();
        family(&families, "kv_rpc_bigtable_reads_finished_total")
            .get_metric()
            .iter()
            .filter(|m| {
                m.get_label()
                    .iter()
                    .any(|label| label.name() == "outcome" && label.value() == outcome)
            })
            .map(|m| m.get_counter().value() as u64)
            .sum()
    }

    /// `gate_stream` should hold the permit until the wrapped stream is
    /// dropped. With a capacity-1 semaphore, the wrapped stream's existence
    /// blocks any other acquire, even between yields. Reading the inner
    /// stream to its end records the read as completed.
    #[tokio::test(start_paused = true)]
    async fn gate_stream_holds_permit_for_full_drain() {
        let limiter = Arc::new(Semaphore::new(1));
        let (permit, registry) = test_permit(&limiter).await;
        assert_eq!(limiter.available_permits(), 0);

        let inner: BoxStream<'static, Result<u64, anyhow::Error>> =
            stream::iter(vec![Ok(1u64), Ok(2), Ok(3)]).boxed();
        let mut wrapped = gate_stream(permit, inner);

        for expected in 1u64..=3 {
            let item = wrapped.try_next().await.unwrap().unwrap();
            assert_eq!(item, expected);
            assert_eq!(
                limiter.available_permits(),
                0,
                "permit should still be held mid-drain"
            );
        }
        assert!(wrapped.try_next().await.unwrap().is_none());
        // Dropping the stream releases the permit.
        drop(wrapped);
        assert_eq!(limiter.available_permits(), 1);
        assert_eq!(reads_finished(&registry, "completed"), 1);
        assert_eq!(reads_finished(&registry, "cancelled"), 0);
    }

    /// Dropping the wrapped stream early — before the inner stream is
    /// drained — should release the permit immediately and record the read
    /// as cancelled.
    #[tokio::test(start_paused = true)]
    async fn gate_stream_releases_permit_on_early_drop() {
        let limiter = Arc::new(Semaphore::new(1));
        let (permit, registry) = test_permit(&limiter).await;
        assert_eq!(limiter.available_permits(), 0);

        // Wrap an infinite stream so we can drop in the middle.
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let inner: BoxStream<'static, Result<u64, anyhow::Error>> =
            stream::repeat_with(move || {
                let n = counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok(n as u64)
            })
            .boxed();
        let mut wrapped = gate_stream(permit, inner);

        let _ = wrapped.try_next().await.unwrap().unwrap();
        let _ = wrapped.try_next().await.unwrap().unwrap();
        assert_eq!(limiter.available_permits(), 0);

        drop(wrapped);
        // The drop is synchronous; the permit is released immediately.
        // (`tokio::sync::Semaphore` doesn't need yield-to-runtime to observe
        // the release.)
        tokio::time::sleep(Duration::from_millis(0)).await;
        assert_eq!(limiter.available_permits(), 1);
        assert_eq!(reads_finished(&registry, "cancelled"), 1);
        assert_eq!(reads_finished(&registry, "completed"), 0);
    }

    /// Inner errors should propagate through unchanged; the permit should
    /// still release on the resulting stream-end, and the read is recorded
    /// as failed rather than cancelled.
    #[tokio::test(start_paused = true)]
    async fn gate_stream_propagates_inner_error() {
        let limiter = Arc::new(Semaphore::new(1));
        let (permit, registry) = test_permit(&limiter).await;

        let inner: BoxStream<'static, Result<u64, anyhow::Error>> =
            stream::iter(vec![Ok::<u64, anyhow::Error>(1u64), Err(anyhow!("boom"))]).boxed();
        let mut wrapped = gate_stream(permit, inner);

        assert_eq!(wrapped.try_next().await.unwrap().unwrap(), 1);
        let err = wrapped.try_next().await.unwrap_err();
        assert!(err.to_string().contains("boom"), "got: {err}");

        drop(wrapped);
        tokio::time::sleep(Duration::from_millis(0)).await;
        assert_eq!(limiter.available_permits(), 1);
        assert_eq!(reads_finished(&registry, "failed"), 1);
        assert_eq!(reads_finished(&registry, "cancelled"), 0);
    }

    /// A read that returns its whole result records completed when it
    /// returns, and cancelled when its future is dropped with the read in
    /// flight.
    #[tokio::test]
    async fn whole_result_read_records_completed_and_cancelled() {
        let (ctx, registry, mock, server) =
            crate::v2::test_utils::query_context_with_mock_and_registry("list_events", 1).await;
        let checkpoint = sui_types::test_checkpoint_data_builder::TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .finish_transaction()
            .build_checkpoint();
        sui_kvstore::testing::insert_checkpoint_rows(&mock, &checkpoint).await;
        let client = ctx.client();

        let row = client
            .tx_seq_digest_row(0)
            .await
            .unwrap()
            .expect("transaction 0 has a row");
        assert_eq!((row.checkpoint_number, row.tx_offset), (0, 0));
        assert_eq!(reads_finished(&registry, "completed"), 1);

        // The mock holds its response far longer than the timeout, so the
        // read is still in flight when the timeout drops it.
        mock.set_read_rows_latency(Duration::from_secs(60)).await;
        let in_flight =
            tokio::time::timeout(Duration::from_millis(50), client.tx_seq_digest_row(0)).await;
        assert!(in_flight.is_err(), "the read should still be in flight");
        assert_eq!(reads_finished(&registry, "cancelled"), 1);

        let families = registry.gather();
        let started: f64 = family(&families, "kv_rpc_bigtable_reads_started_total")
            .get_metric()
            .iter()
            .map(|m| m.get_counter().value())
            .sum();
        assert_eq!(started, 2.0);
        server.abort();
    }

    /// Frontiers are placed by the one `tx_seq_digest` row their
    /// [`RowPlacement`] names: ascending by the transaction before the boundary,
    /// descending by the one after it. A frontier's row is always indexed.
    #[tokio::test]
    async fn places_frontiers_by_one_row() {
        let (ctx, _registry, mock, server) =
            crate::v2::test_utils::query_context_with_mock_and_registry("list_events", 2).await;
        // Checkpoint 0 holds transactions 0 and 1, checkpoint 1 transaction 2.
        let mut builder = sui_types::test_checkpoint_data_builder::TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .finish_transaction()
            .start_transaction(1)
            .finish_transaction();
        let first = builder.build_checkpoint();
        let second = builder
            .start_transaction(2)
            .finish_transaction()
            .build_checkpoint();
        for checkpoint in [&first, &second] {
            sui_kvstore::testing::insert_checkpoint_rows(&mock, checkpoint).await;
        }
        let client = ctx.client();
        let ascending = ScanDirection::Ascending;
        let descending = ScanDirection::Descending;
        let placed = |checkpoint, point| Some(PlacedFrontier { checkpoint, point });

        for (placement, expected) in [
            // After transaction 1, the last of checkpoint 0: an index past its end.
            (
                RowPlacement::transaction_boundary(2, ascending),
                placed(0, LedgerPoint::transaction(0, 2)),
            ),
            (
                RowPlacement::transaction_boundary(2, descending),
                placed(1, LedgerPoint::transaction(1, 0)),
            ),
            (RowPlacement::transaction_boundary(0, ascending), None),
            (
                RowPlacement::event_boundary(
                    IntraTxCoordinate {
                        tx_seq: 1,
                        index: 3,
                    },
                    ascending,
                ),
                placed(0, LedgerPoint::event(0, 1, 3)),
            ),
        ] {
            assert_eq!(
                client.place_frontier(placement).await.unwrap(),
                expected,
                "{placement:?}"
            );
        }
        let missing = client
            .place_frontier(RowPlacement::transaction_boundary(9, descending))
            .await
            .expect_err("a frontier's row is always indexed");
        assert_eq!(
            missing.into_status_proto().code,
            tonic::Code::Internal as i32
        );
        server.abort();
    }

    /// Find the metric family with the given name in a gathered registry
    /// snapshot, panicking if absent.
    fn family<'a>(families: &'a [MetricFamily], name: &str) -> &'a MetricFamily {
        families
            .iter()
            .find(|f| f.name() == name)
            .unwrap_or_else(|| panic!("metric family {name} not registered"))
    }

    /// Total observation count summed across every label series in a
    /// histogram family.
    fn histogram_sample_count(family: &MetricFamily) -> u64 {
        family
            .get_metric()
            .iter()
            .map(|m| m.get_histogram().get_sample_count())
            .sum()
    }

    /// Build a `BigTableClientContext` directly without instantiating a real
    /// `BigTableClient`. Tests exercise limiter accounting and `Drop` against
    /// the context, which is sufficient; the rest of `BigTableClient` is thin
    /// passthrough to `sui_kvstore::BigTableClient`.
    fn test_context(metrics: Arc<Metrics>, method: &'static str) -> Arc<BigTableClientContext> {
        Arc::new(BigTableClientContext {
            metrics,
            method,
            active_permits: AtomicUsize::new(0),
            peak_used: AtomicUsize::new(0),
            ops_count: AtomicUsize::new(0),
        })
    }

    #[test]
    fn record_acquired_tracks_active_peak_and_release() {
        let (metrics, registry) = Metrics::for_testing();
        let context = test_context(metrics, "list_transactions");

        record_acquired(&context, stage::TRANSACTIONS, Duration::from_millis(7));
        assert_eq!(context.ops_count.load(Ordering::Relaxed), 1);
        assert_eq!(context.active_permits.load(Ordering::Relaxed), 1);
        assert_eq!(context.peak_used.load(Ordering::Relaxed), 1);

        record_acquired(&context, stage::OBJECTS, Duration::from_millis(3));
        assert_eq!(context.ops_count.load(Ordering::Relaxed), 2);
        assert_eq!(context.active_permits.load(Ordering::Relaxed), 2);
        assert_eq!(context.peak_used.load(Ordering::Relaxed), 2);

        record_release(&context);
        assert_eq!(context.active_permits.load(Ordering::Relaxed), 1);
        assert_eq!(context.peak_used.load(Ordering::Relaxed), 2);
        record_release(&context);
        assert_eq!(context.active_permits.load(Ordering::Relaxed), 0);

        drop(context);
        let families = registry.gather();
        assert_eq!(
            histogram_sample_count(family(&families, "kv_rpc_bigtable_permit_wait_ms")),
            2
        );
        assert_eq!(
            histogram_sample_count(family(&families, "kv_rpc_bigtable_permits_peak")),
            1,
            "permits_peak observed once on context drop"
        );
        assert_eq!(
            histogram_sample_count(family(&families, "kv_rpc_bigtable_ops_total")),
            1,
            "ops_total observed once on context drop"
        );
    }

    #[tokio::test]
    async fn limited_permit_records_hold_once_on_drop() {
        let (metrics, registry) = Metrics::for_testing();
        let context = test_context(metrics, "list_transactions");
        let limiter = Arc::new(Semaphore::new(1));

        let permit = acquire_limited(limiter.clone(), context.clone(), stage::TX_CP_RESOLVE)
            .await
            .unwrap();
        assert_eq!(limiter.available_permits(), 0);

        drop(permit);
        assert_eq!(limiter.available_permits(), 1);
        drop(context);

        let families = registry.gather();
        let permit_hold = family(&families, "kv_rpc_bigtable_permit_hold_ms");
        assert_eq!(histogram_sample_count(permit_hold), 1);
        let labels = permit_hold.get_metric()[0].get_label();
        assert!(
            labels
                .iter()
                .any(|label| label.name() == "method" && label.value() == "list_transactions")
        );
        assert!(
            labels
                .iter()
                .any(|label| label.name() == "stage" && label.value() == "tx_cp_resolve")
        );
    }

    #[test]
    fn stage_labels_are_bounded_and_counted() {
        let (metrics, registry) = Metrics::for_testing();
        let context = test_context(metrics, "list_events");

        record_acquired(&context, stage::TRANSACTIONS, Duration::from_millis(1));
        record_acquired(&context, stage::OBJECTS, Duration::from_millis(2));
        record_release(&context);
        record_release(&context);
        drop(context);

        let families = registry.gather();
        assert_eq!(
            histogram_sample_count(family(&families, "kv_rpc_bigtable_permits_peak")),
            1,
            "permits_peak observed once on context drop"
        );
        assert_eq!(
            histogram_sample_count(family(&families, "kv_rpc_bigtable_ops_total")),
            1,
            "ops_total observed once on context drop"
        );
        assert_eq!(
            histogram_sample_count(family(&families, "kv_rpc_bigtable_permit_wait_ms")),
            2,
            "one permit_wait observation per successful acquire"
        );

        let permit_wait = family(&families, "kv_rpc_bigtable_permit_wait_ms");
        let by_stage: std::collections::BTreeMap<String, u64> = permit_wait
            .get_metric()
            .iter()
            .map(|m| {
                let stage = m
                    .get_label()
                    .iter()
                    .find(|l| l.name() == "stage")
                    .unwrap()
                    .value()
                    .to_string();
                (stage, m.get_histogram().get_sample_count())
            })
            .collect();
        assert_eq!(by_stage.get("transactions"), Some(&1));
        assert_eq!(by_stage.get("objects"), Some(&1));
    }
}
