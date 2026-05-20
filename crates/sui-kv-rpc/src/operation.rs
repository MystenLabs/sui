// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::time::Duration;

use futures::Stream;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_inverted_index::BitmapQuery;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::tables::event_bitmap_index;
use sui_kvstore::tables::transaction_bitmap_index;
use sui_rpc::proto::sui::rpc::v2alpha::EventFilter;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionFilter;
use sui_rpc_api::RpcError;
use tokio::time::Instant;

use crate::KvRpcMetrics;
use crate::KvRpcServer;
use crate::PackageResolver;
use crate::bigtable_client::BigTableClient;
use crate::bigtable_client::ConcurrencyConfig;
use crate::filter::event_filter_to_query;
use crate::filter::transaction_filter_to_query;

#[derive(Clone, Copy, Debug)]
pub(crate) struct OperationSpec {
    pub(crate) name: &'static str,
    pub(crate) timeout: Duration,
}

impl OperationSpec {
    pub(crate) const fn new(name: &'static str, timeout: Duration) -> Self {
        Self { name, timeout }
    }
}

#[derive(Clone)]
pub(crate) struct QueryContext {
    client: BigTableClient,
    package_resolver: PackageResolver,
    metrics: std::sync::Arc<KvRpcMetrics>,
    method: &'static str,
    checkpoint_hi_exclusive: u64,
    limits: ConcurrencyConfig,
}

impl QueryContext {
    pub(crate) fn new(
        client: BigTableClient,
        package_resolver: PackageResolver,
        metrics: std::sync::Arc<KvRpcMetrics>,
        method: &'static str,
        checkpoint_hi_exclusive: u64,
        limits: ConcurrencyConfig,
    ) -> Self {
        Self {
            client,
            package_resolver,
            metrics,
            method,
            checkpoint_hi_exclusive,
            limits,
        }
    }

    pub(crate) fn client(&self) -> &BigTableClient {
        &self.client
    }

    pub(crate) fn package_resolver(&self) -> &PackageResolver {
        &self.package_resolver
    }

    pub(crate) fn checkpoint_hi_exclusive(&self) -> u64 {
        self.checkpoint_hi_exclusive
    }

    pub(crate) fn request_bigtable_concurrency(&self) -> usize {
        self.limits.request_bigtable_concurrency
    }

    pub(crate) fn response_render_concurrency(&self) -> usize {
        std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(4)
    }

    /// Per-request evaluated-bucket budget for `spec`. Handlers pass this
    /// into `eval_bitmap_query_stream`, which constructs a
    /// `BitmapScanBudget` internally and reports the resulting
    /// `BitmapScanMetrics` via the `on_metrics` callback. The budget caps
    /// evaluated buckets, not backend reads — see
    /// `eval_bitmap_query_stream` for the (≤ leaf_count) slop at
    /// exhaustion.
    pub(crate) fn scan_budget(&self, spec: BitmapIndexSpec) -> u64 {
        match spec.table_name {
            transaction_bitmap_index::NAME => self.limits.bitmap_bucket_budget_tx,
            event_bitmap_index::NAME => self.limits.bitmap_bucket_budget_event,
            other => panic!("unknown bitmap index table {other}; add a budget for it"),
        }
    }

    pub(crate) fn observe_response_render(&self, elapsed: std::time::Duration) {
        self.metrics.observe_response_render(self.method, elapsed);
    }

    pub(crate) fn observe_stage(&self, stage: &'static str, elapsed: std::time::Duration) {
        self.metrics
            .observe_stage_latency(self.method, stage, elapsed);
    }

    pub(crate) fn observe_stream_item_yield_wait(&self, elapsed: std::time::Duration) {
        self.metrics
            .observe_stream_item_yield_wait(self.method, elapsed);
    }

    pub(crate) fn transaction_filter_query(
        &self,
        filter: &TransactionFilter,
    ) -> Result<BitmapQuery, RpcError> {
        transaction_filter_to_query(filter, self.limits.max_bitmap_filter_literals)
    }

    pub(crate) fn event_filter_query(&self, filter: &EventFilter) -> Result<BitmapQuery, RpcError> {
        event_filter_to_query(filter, self.limits.max_bitmap_filter_literals)
    }

    /// Callback for `eval_bitmap_query_stream`'s `on_metrics`. Fires exactly
    /// once when the eval pipeline drops (natural end, error, or consumer
    /// cancel) and records `buckets_evaluated` to the
    /// `kv_rpc_bitmap_buckets_evaluated` histogram for budget tuning. Also
    /// emits a debug log line for ad-hoc inspection.
    pub(crate) fn bitmap_scan_observer(
        &self,
    ) -> impl FnOnce(sui_inverted_index::BitmapScanMetrics) + Send + 'static {
        let metrics = self.metrics.clone();
        let method = self.method;
        move |m| {
            metrics.observe_bitmap_buckets_evaluated(method, m.buckets_evaluated);
            tracing::debug!(
                method,
                buckets_evaluated = m.buckets_evaluated,
                "bitmap scan complete"
            );
        }
    }
}

/// Wrap a server-streaming response with a wall-clock deadline.
///
/// Guarantee: when the deadline fires, the inner stream is dropped and
/// its resources (Bigtable permits, in-flight RPCs, render buffers) are
/// released in real time — even if the gRPC consumer has stopped pulling
/// frames. The `DeadlineExceeded` Status itself is delivered on the next
/// poll from tonic, which may be later if the h2 send window is closed.
///
/// The naive design — race deadline and `inner.next()` in a single
/// `select!` inside the wrapper — fails when tonic's task is parked at
/// its h2-write await: timer wakes hit the task but resume at the wrong
/// await point, and the wrapper's select never runs. Spawning gives the
/// drain loop its own task whose only outer await is `timeout_at(...)`,
/// so deadline wakes always land where they can cancel.
///
/// The mpsc(1) channel is just the bridge between two polling roots
/// (tonic ↔ our spawn). Capacity 1 = tightest backpressure; per-item
/// wake overhead is negligible against IO/render cost.
fn with_deadline<S, T>(
    stream: S,
    deadline: Instant,
    operation: &'static str,
) -> BoxStream<'static, Result<T, tonic::Status>>
where
    S: Stream<Item = Result<T, tonic::Status>> + Send + 'static,
    T: Send + 'static,
{
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<T, tonic::Status>>(1);

    // Spawn the drain loop. `timeout_at` is the outermost await of the
    // task, so a deadline wake always lands inside it and observes
    // Ready, regardless of which inner await (next / send) is suspended.
    let producer = tokio::spawn(async move {
        let _ = tokio::time::timeout_at(deadline, async move {
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                // Consumer dropped → channel closed → stop work.
                if tx.send(item).await.is_err() {
                    return;
                }
            }
        })
        .await;
    });

    // Synchronous abort if the wrapper is dropped before its body runs
    // (or while suspended inside `inner.next()` rather than `send`).
    struct AbortOnDrop(tokio::task::AbortHandle);
    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let abort_guard = AbortOnDrop(producer.abort_handle());

    // Lift the select! result into an enum so the macro's elaboration
    // can infer the try_stream's error type from the match arms.
    enum Step<T> {
        Item(Option<Result<T, tonic::Status>>),
        Deadline,
    }

    async_stream::try_stream! {
        // Move the guard into the generator so dropping the unpolled
        // stream still drops it (and thus aborts the producer).
        let _abort_on_drop = abort_guard;
        // Held until the consumer drains the channel — then awaited to
        // surface any panic from the producer task as an Internal error.
        let mut producer = producer;
        let sleep = tokio::time::sleep_until(deadline);
        futures::pin_mut!(sleep);
        loop {
            // `biased`: past-deadline polls emit DeadlineExceeded
            // promptly without waiting on a buffered item.
            let step = tokio::select! {
                biased;
                _ = &mut sleep => Step::Deadline,
                item = rx.recv() => Step::Item(item),
            };
            match step {
                Step::Item(Some(Ok(it))) => yield it,
                Step::Item(Some(Err(e))) => Err(e)?,
                Step::Item(None) => {
                    // Producer closed the channel — either natural EOF or
                    // a panic that aborted the task before EOF. Distinguish
                    // by awaiting the JoinHandle: a panic surfaces as an
                    // Internal error so the consumer doesn't see truncated
                    // success. The panic message itself is logged by the
                    // global `telemetry-subscribers` panic hook (the boxed
                    // payload here is an opaque Rust-internal type that
                    // can't be cheaply downcast to a string), so the wire
                    // status carries only a generic marker.
                    //
                    // TODO: once kv-rpc adds a CatchPanicLayer to its Tower
                    // stack (sister services already do), this translation
                    // can move there and we can just `resume_unwind` here.
                    match (&mut producer).await {
                        Ok(()) => break,
                        Err(e) if e.is_panic() => {
                            tracing::error!(operation, "producer task panicked");
                            Err(tonic::Status::internal(format!(
                                "{operation} request panicked"
                            )))?;
                        }
                        Err(_) => {
                            // Cancellation — only possible if the abort
                            // guard fired (which only happens on Drop), so
                            // we shouldn't be polling. Treat as EOF.
                            break;
                        }
                    }
                }
                Step::Deadline => {
                    tracing::warn!(operation, "request deadline exceeded");
                    Err(tonic::Status::deadline_exceeded(format!(
                        "{operation} request deadline exceeded"
                    )))?;
                }
            }
        }
    }
    .boxed()
}

impl KvRpcServer {
    fn limited_client(&self, operation: &'static str) -> BigTableClient {
        BigTableClient::new(
            self.client.clone(),
            self.concurrency.request_bigtable_concurrency,
            self.metrics.bigtable_limiter.clone(),
            operation,
        )
    }

    async fn cached_checkpoint_hi_exclusive(&self) -> Result<u64, RpcError> {
        let checkpoint_hi_inclusive = {
            let cache = self.cache.read().await;
            cache.as_ref().and_then(|info| info.checkpoint_height)
        }
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Unavailable,
                "service info cache missing checkpoint height",
            )
        })?;

        checkpoint_hi_inclusive.checked_add(1).ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                "cached checkpoint height cannot be represented as an exclusive bound",
            )
        })
    }

    async fn query_context(&self, operation: &'static str) -> Result<QueryContext, RpcError> {
        Ok(QueryContext::new(
            self.limited_client(operation),
            self.package_resolver.clone(),
            self.metrics.clone(),
            operation,
            self.cached_checkpoint_hi_exclusive().await?,
            self.concurrency,
        ))
    }

    pub(crate) async fn serve_query_stream<Req, T, F, Fut>(
        &self,
        spec: OperationSpec,
        request: tonic::Request<Req>,
        handler: F,
    ) -> Result<tonic::Response<BoxStream<'static, Result<T, tonic::Status>>>, tonic::Status>
    where
        Req: Send + 'static,
        T: Send + 'static,
        F: FnOnce(QueryContext, Req) -> Fut + Send,
        Fut: Future<Output = Result<BoxStream<'static, Result<T, RpcError>>, RpcError>> + Send,
    {
        let deadline = Instant::now() + spec.timeout;
        let request = request.into_inner();

        let stream = tokio::time::timeout_at(deadline, async move {
            let ctx = self.query_context(spec.name).await?;
            handler(ctx, request).await
        })
        .await
        .map_err(|_| {
            tracing::warn!(operation = spec.name, "construction phase timed out");
            tonic::Status::deadline_exceeded(format!("{} request deadline exceeded", spec.name))
        })?
        .map_err(tonic::Status::from)?;

        let stream = stream.map_err(tonic::Status::from);
        Ok(tonic::Response::new(with_deadline(
            stream, deadline, spec.name,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering as AtomicOrdering;

    /// `start_paused = true` makes `tokio::time` virtual: sleeps and
    /// `Instant::now()` advance only when the runtime explicitly waits, so
    /// the test runs instantly and deterministically.
    #[tokio::test(start_paused = true)]
    async fn with_deadline_emits_deadline_exceeded_when_inner_hangs() {
        let inner: BoxStream<'static, Result<u64, tonic::Status>> = stream::pending().boxed();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bounded = with_deadline(inner, deadline, "test");

        let item = bounded.next().await;
        let status = item.expect("got an item").expect_err("got a status error");
        assert_eq!(status.code(), tonic::Code::DeadlineExceeded);
        assert!(bounded.next().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn with_deadline_passes_items_through_until_deadline() {
        let inner = stream::iter([Ok::<_, tonic::Status>(1), Ok(2)]).boxed();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bounded = with_deadline(inner, deadline, "test");

        assert_eq!(bounded.next().await.unwrap().unwrap(), 1);
        assert_eq!(bounded.next().await.unwrap().unwrap(), 2);
        assert!(bounded.next().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn with_deadline_propagates_inner_error_before_deadline() {
        let inner = stream::iter([Err::<u64, _>(tonic::Status::unavailable("nope"))]).boxed();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bounded = with_deadline(inner, deadline, "test");

        let status = bounded.next().await.unwrap().unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unavailable);
    }

    /// Counts inner-stream yields to prove the spawned producer is dropped
    /// at deadline-time even when the consumer never polls. If the deadline
    /// were only observed on the consumer's next poll, the counter would
    /// keep growing while virtual time advances. Defends the contract that
    /// `with_deadline` cancels in-flight work in wall-clock time regardless
    /// of consumer pace.
    #[tokio::test(start_paused = true)]
    async fn with_deadline_drops_inner_when_consumer_is_slow_past_deadline() {
        let count = Arc::new(AtomicU64::new(0));
        let inner: BoxStream<'static, Result<u64, tonic::Status>> = {
            let count = count.clone();
            stream::unfold((), move |()| {
                let count = count.clone();
                async move {
                    count.fetch_add(1, AtomicOrdering::SeqCst);
                    Some((Ok::<u64, tonic::Status>(1), ()))
                }
            })
            .boxed()
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bounded = with_deadline(inner, deadline, "test");

        // Drain one item; producer is now blocked on a full channel
        // somewhere past it.
        assert_eq!(bounded.next().await.unwrap().unwrap(), 1);

        // Stop polling. Advance virtual time past the deadline. The
        // producer-side `timeout_at` must drop the inner stream here.
        tokio::time::sleep(Duration::from_secs(10)).await;
        let snapshot = count.load(AtomicOrdering::SeqCst);

        // Past-deadline: no further inner-stream yields should be observed.
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(
            count.load(AtomicOrdering::SeqCst),
            snapshot,
            "inner stream kept producing past the deadline",
        );

        // And the consumer sees `DeadlineExceeded` on its next poll.
        let status = bounded.next().await.unwrap().unwrap_err();
        assert_eq!(status.code(), tonic::Code::DeadlineExceeded);
    }

    /// A panic inside the producer task must surface as `Internal`
    /// instead of silently closing the channel (which the consumer can't
    /// distinguish from a clean EOF). Without this translation, a
    /// truncated response looks like a successful one to the client.
    #[tokio::test(start_paused = true)]
    async fn with_deadline_translates_producer_panic_to_internal() {
        let inner: BoxStream<'static, Result<u64, tonic::Status>> =
            stream::unfold(0u64, |i| async move {
                if i == 1 {
                    panic!("boom from inner stream");
                }
                Some((Ok::<u64, tonic::Status>(i), i + 1))
            })
            .boxed();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut bounded = with_deadline(inner, deadline, "test");

        // First item flows through normally.
        assert_eq!(bounded.next().await.unwrap().unwrap(), 0);
        // Next pull observes the producer panic via the JoinHandle.
        let status = bounded.next().await.unwrap().unwrap_err();
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    /// Dropping the wrapper before any item is drained must abort the
    /// spawned producer and drop the inner stream — otherwise a client
    /// that hangs up early would leak the in-flight pipeline until its
    /// own deadline.
    #[tokio::test(start_paused = true)]
    async fn with_deadline_aborts_producer_when_consumer_drops() {
        struct DropBeacon(Arc<AtomicBool>);
        impl Drop for DropBeacon {
            fn drop(&mut self) {
                self.0.store(true, AtomicOrdering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let beacon = DropBeacon(dropped.clone());
        // Stream that never yields but holds the beacon for its lifetime —
        // beacon's Drop fires iff the producer task drops the inner stream.
        let inner: BoxStream<'static, Result<u64, tonic::Status>> =
            stream::unfold(beacon, |state| async move {
                let _hold = &state;
                std::future::pending::<()>().await;
                Some((Ok::<u64, tonic::Status>(1), state))
            })
            .boxed();
        let deadline = Instant::now() + Duration::from_secs(60);

        let bounded = with_deadline(inner, deadline, "test");
        drop(bounded);

        // Give the runtime cycles to deliver the abort and drop the
        // producer task's future.
        for _ in 0..10 {
            tokio::task::yield_now().await;
            if dropped.load(AtomicOrdering::SeqCst) {
                break;
            }
        }
        assert!(
            dropped.load(AtomicOrdering::SeqCst),
            "inner stream was not dropped after consumer dropped the wrapper",
        );
    }
}
