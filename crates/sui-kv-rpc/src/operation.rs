// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::time::Duration;

use futures::Stream;
use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_inverted_index::BitmapQuery;
use sui_rpc_api::RpcError;
use tokio::time::Instant;

use crate::KvRpcMetrics;
use crate::KvRpcServer;
use crate::PackageResolver;
use crate::bigtable_client::BigTableClient;
use crate::bigtable_client::ConcurrencyConfig;
use crate::filter::event_filter_to_query;
use crate::filter::transaction_filter_to_query;
use sui_rpc::proto::sui::rpc::v2alpha::EventFilter;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionFilter;

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

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReadLimits {
    request_bigtable_concurrency: usize,
    max_bitmap_filter_literals: usize,
}

impl From<ConcurrencyConfig> for ReadLimits {
    fn from(config: ConcurrencyConfig) -> Self {
        Self {
            request_bigtable_concurrency: config.request_bigtable_concurrency,
            max_bitmap_filter_literals: config.max_bitmap_filter_literals,
        }
    }
}

#[derive(Clone)]
pub(crate) struct QueryContext {
    client: BigTableClient,
    package_resolver: PackageResolver,
    metrics: std::sync::Arc<KvRpcMetrics>,
    method: &'static str,
    checkpoint_hi_exclusive: u64,
    limits: ReadLimits,
}

impl QueryContext {
    pub(crate) fn new(
        client: BigTableClient,
        package_resolver: PackageResolver,
        metrics: std::sync::Arc<KvRpcMetrics>,
        method: &'static str,
        checkpoint_hi_exclusive: u64,
        limits: ReadLimits,
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

    pub(crate) fn observe_response_render(&self, elapsed: std::time::Duration) {
        self.metrics.observe_response_render(self.method, elapsed);
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
}

/// Wrap a server-streaming response with an absolute deadline. When the
/// deadline fires, the wrapped stream yields a `DeadlineExceeded` Status
/// once and ends; the inner stream is dropped at that point.
fn with_deadline<S, T>(
    stream: S,
    deadline: Instant,
    operation: &'static str,
) -> BoxStream<'static, Result<T, tonic::Status>>
where
    S: Stream<Item = Result<T, tonic::Status>> + Send + 'static,
    T: Send + 'static,
{
    enum Step<T> {
        Item(Option<Result<T, tonic::Status>>),
        Deadline,
    }

    async_stream::try_stream! {
        let sleep = tokio::time::sleep_until(deadline);
        futures::pin_mut!(stream);
        futures::pin_mut!(sleep);
        loop {
            let step = tokio::select! {
                biased;
                item = stream.next() => Step::Item(item),
                _ = &mut sleep => Step::Deadline,
            };
            match step {
                Step::Item(Some(Ok(it))) => yield it,
                Step::Item(Some(Err(e))) => Err(e)?,
                Step::Item(None) => break,
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
            ReadLimits::from(self.concurrency),
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
}
