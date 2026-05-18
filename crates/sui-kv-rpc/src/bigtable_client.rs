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
use prometheus::Registry;
use prometheus::register_histogram_vec_with_registry;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::ScanDirection;
use sui_inverted_index::eval_bitmap_query_stream;
use sui_kvstore::BigTableBitmapSource;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::RowFilter;
use sui_kvstore::TransactionData;
use sui_kvstore::TxSeqDigestData;
use sui_rpc_api::RpcError;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;

/// Read-side concurrency budgets for v2alpha list APIs.
#[derive(Clone, Copy, Debug)]
pub struct ConcurrencyConfig {
    /// Per-request semaphore capacity gating downstream BigTable reads on
    /// `BigTableClient`.
    pub request_bigtable_concurrency: usize,
    /// Maximum bitmap scan fanout accepted in one filter request. Each
    /// literal can become one bitmap dimension stream; bitmap scans do not
    /// consume downstream request permits.
    pub max_bitmap_filter_literals: usize,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            request_bigtable_concurrency: 10,
            max_bitmap_filter_literals: 10,
        }
    }
}

impl ConcurrencyConfig {
    /// Reject configurations that cannot make forward progress.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.request_bigtable_concurrency > 0,
            "request_bigtable_concurrency must be greater than zero",
        );
        anyhow::ensure!(
            self.max_bitmap_filter_literals > 0,
            "max_bitmap_filter_literals must be greater than zero",
        );
        Ok(())
    }
}

/// Stage label values for the `permit_wait_ms` metric. `&'static str` so they
/// satisfy `with_label_values` zero-allocation.
pub(crate) mod stage {
    pub const TRANSACTIONS: &str = "transactions";
    pub const OBJECTS: &str = "objects";
    pub const TX_SEQ_DIGEST: &str = "tx_seq_digest";
    pub const CHECKPOINTS: &str = "checkpoints";
}

/// Histograms for tuning the request-scoped `BigTableClient` semaphore.
/// Cardinality is bounded: 3 methods × 4 stages = 12 series for
/// `permit_wait_ms`, 3 series for the request-end `permits_peak` and
/// `ops_total` histograms.
#[derive(Clone)]
pub(crate) struct Metrics {
    /// Time spent waiting for one limiter permit, recorded once per successful
    /// `acquire_owned()` call.
    pub permit_wait_ms: HistogramVec,
    /// Peak in-use permit count observed during a single request, recorded
    /// once when the request's `BigTableClient` is dropped.
    pub permits_peak: HistogramVec,
    /// Total limiter acquisitions issued by a single request, recorded once
    /// when the request's `BigTableClient` is dropped.
    pub ops_total: HistogramVec,
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
            permits_peak: register_histogram_vec_with_registry!(
                "kv_rpc_bigtable_permits_peak",
                "Peak in-use BigTable limiter permits observed during a single v2alpha request.",
                &["method"],
                vec![1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 50.0],
                registry,
            )
            .unwrap(),
            ops_total: register_histogram_vec_with_registry!(
                "kv_rpc_bigtable_ops_total",
                "Total BigTable limiter acquisitions issued by a single v2alpha request.",
                &["method"],
                prometheus::exponential_buckets(1.0, 2.0, 14).unwrap(),
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
        let permit = self.acquire(stage).await?;
        let inner = self
            .inner
            .clone()
            .multi_get_stream(table_name, keys, filter)
            .await
            .map_err(RpcError::from)?;
        Ok(gate_stream(permit, inner.boxed()))
    }

    pub(crate) async fn get_transactions_stream(
        &self,
        digests: Vec<TransactionDigest>,
        column_filter: Option<RowFilter>,
    ) -> Result<
        BoxStream<'static, Result<(TransactionDigest, TransactionData), anyhow::Error>>,
        RpcError,
    > {
        let permit = self.acquire(stage::TRANSACTIONS).await?;
        let inner = self
            .inner
            .clone()
            .get_transactions_stream(digests, column_filter)
            .await
            .map_err(RpcError::from)?;
        Ok(gate_stream(permit, inner.boxed()))
    }

    pub(crate) async fn get_objects_stream(
        &self,
        object_keys: Vec<ObjectKey>,
    ) -> Result<BoxStream<'static, Result<Object, anyhow::Error>>, RpcError> {
        let permit = self.acquire(stage::OBJECTS).await?;
        let inner = self
            .inner
            .clone()
            .get_objects_stream(object_keys)
            .await
            .map_err(RpcError::from)?;
        Ok(gate_stream(permit, inner.boxed()))
    }

    pub(crate) async fn resolve_tx_digests_stream(
        &self,
        tx_sequence_numbers: Vec<u64>,
    ) -> Result<BoxStream<'static, Result<TxSeqDigestData, anyhow::Error>>, RpcError> {
        let permit = self.acquire(stage::TX_SEQ_DIGEST).await?;
        let inner = self
            .inner
            .clone()
            .resolve_tx_digests_stream(tx_sequence_numbers)
            .await
            .map_err(RpcError::from)?;
        Ok(gate_stream(permit, inner.boxed()))
    }

    pub(crate) async fn checkpoint_to_tx_range(
        &self,
        checkpoint_range: Range<u64>,
    ) -> Result<Range<u64>, RpcError> {
        let _permit = self.acquire(stage::CHECKPOINTS).await?;
        self.inner
            .clone()
            .checkpoint_to_tx_range(checkpoint_range)
            .await
            .map_err(RpcError::from)
    }

    pub(crate) async fn resolve_tx_checkpoints(
        &self,
        tx_sequence_numbers: &[u64],
    ) -> Result<Vec<(u64, CheckpointSequenceNumber)>, RpcError> {
        let _permit = self.acquire(stage::CHECKPOINTS).await?;
        self.inner
            .clone()
            .resolve_tx_checkpoints(tx_sequence_numbers)
            .await
            .map_err(RpcError::from)
    }

    /// Eval a `BitmapQuery`. Bitmap scans stay outside the downstream request
    /// semaphore; their concurrency is bounded by `max_bitmap_filter_literals`
    /// during filter validation.
    pub(crate) fn eval_bitmap_query_stream(
        &self,
        query: BitmapQuery,
        range: Range<u64>,
        spec: BitmapIndexSpec,
        direction: ScanDirection,
    ) -> impl Stream<Item = Result<u64, anyhow::Error>> + Send + 'static {
        let source = BigTableBitmapSource::new(self.inner.clone(), spec);
        eval_bitmap_query_stream(source, query, range, spec.bucket_size, direction)
    }

    async fn acquire(&self, stage: &'static str) -> Result<LimitedPermit, RpcError> {
        acquire_limited(self.limiter.clone(), self.context.clone(), stage)
            .await
            .map_err(|_| RpcError::new(tonic::Code::Internal, "request concurrency limiter closed"))
    }
}

struct LimitedPermit {
    _permit: OwnedSemaphorePermit,
    context: Arc<BigTableClientContext>,
}

impl Drop for LimitedPermit {
    fn drop(&mut self) {
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
    record_acquired(&context, stage, start.elapsed());
    Ok(LimitedPermit {
        _permit: permit,
        context,
    })
}

fn record_acquired(context: &BigTableClientContext, stage: &'static str, wait: Duration) {
    let wait_ms = wait.as_secs_f64() * 1000.0;
    context
        .metrics
        .permit_wait_ms
        .with_label_values(&[context.method, stage])
        .observe(wait_ms);
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
/// propagating through `Drop`). The returned stream is a thin pass-through —
/// items and errors flow through unchanged.
fn gate_stream<T>(
    permit: impl Send + 'static,
    inner: BoxStream<'static, Result<T, anyhow::Error>>,
) -> BoxStream<'static, Result<T, anyhow::Error>>
where
    T: Send + 'static,
{
    async_stream::try_stream! {
        let _permit = permit;
        futures::pin_mut!(inner);
        while let Some(row) = inner.next().await {
            yield row?;
        }
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

    /// `gate_stream` should hold the permit until the wrapped stream is
    /// dropped. With a capacity-1 semaphore, the wrapped stream's existence
    /// blocks any other acquire, even between yields.
    #[tokio::test(start_paused = true)]
    async fn gate_stream_holds_permit_for_full_drain() {
        let limiter = Arc::new(Semaphore::new(1));
        let permit = limiter.clone().acquire_owned().await.unwrap();
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
    }

    /// Dropping the wrapped stream early — before the inner stream is
    /// drained — should release the permit immediately.
    #[tokio::test(start_paused = true)]
    async fn gate_stream_releases_permit_on_early_drop() {
        let limiter = Arc::new(Semaphore::new(1));
        let permit = limiter.clone().acquire_owned().await.unwrap();
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
    }

    /// Inner errors should propagate through unchanged; the permit should
    /// still release on the resulting stream-end.
    #[tokio::test(start_paused = true)]
    async fn gate_stream_propagates_inner_error() {
        let limiter = Arc::new(Semaphore::new(1));
        let permit = limiter.clone().acquire_owned().await.unwrap();

        let inner: BoxStream<'static, Result<u64, anyhow::Error>> =
            stream::iter(vec![Ok::<u64, anyhow::Error>(1u64), Err(anyhow!("boom"))]).boxed();
        let mut wrapped = gate_stream(permit, inner);

        assert_eq!(wrapped.try_next().await.unwrap().unwrap(), 1);
        let err = wrapped.try_next().await.unwrap_err();
        assert!(err.to_string().contains("boom"), "got: {err}");

        drop(wrapped);
        tokio::time::sleep(Duration::from_millis(0)).await;
        assert_eq!(limiter.available_permits(), 1);
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
