// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::time::Duration;

use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::SkipPolicy;
use sui_kvstore::BitmapIndexSpec;
use sui_kvstore::tables::event_bitmap_index;
use sui_kvstore::tables::transaction_bitmap_index;
use sui_rpc::proto::sui::rpc::v2::EventFilter;
use sui_rpc::proto::sui::rpc::v2::TransactionFilter;
use sui_rpc_api::RpcError;
use tokio::time::Instant;

use crate::KvRpcMetrics;
use crate::KvRpcServer;
use crate::PackageResolver;
use crate::bigtable_client::BigTableClient;
use crate::config::{LedgerHistoryConfig, PipelineStage, ResolvedStageConfig, StagesConfig};
use sui_rpc_api::ledger_history::filter::event_filter_to_query;
use sui_rpc_api::ledger_history::filter::transaction_filter_to_query;

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
    ledger_history: LedgerHistoryConfig,
    request_bigtable_concurrency: usize,
    stages: StagesConfig,
}

impl QueryContext {
    pub(crate) fn new(
        client: BigTableClient,
        package_resolver: PackageResolver,
        metrics: std::sync::Arc<KvRpcMetrics>,
        method: &'static str,
        checkpoint_hi_exclusive: u64,
        ledger_history: LedgerHistoryConfig,
        request_bigtable_concurrency: usize,
        stages: StagesConfig,
    ) -> Self {
        Self {
            client,
            package_resolver,
            metrics,
            method,
            checkpoint_hi_exclusive,
            ledger_history,
            request_bigtable_concurrency,
            stages,
        }
    }

    pub(crate) fn ledger_history(&self) -> &LedgerHistoryConfig {
        &self.ledger_history
    }

    /// Resolved tunables for one read pipeline stage (chunk size + fan-out).
    pub(crate) fn stage(&self, stage: PipelineStage) -> ResolvedStageConfig {
        self.stages.stage(stage)
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
        self.request_bigtable_concurrency
    }

    /// Per-request evaluated-bucket budget for `spec`. Handlers pass this
    /// into `eval_bitmap_query_stream`, which constructs a
    /// `BitmapScanBudget` internally and reports the resulting
    /// `BitmapScanMetrics` via the `on_metrics` callback. The budget caps
    /// evaluated buckets, not backend reads — see
    /// `eval_bitmap_query_stream` for the (≤ unique_leaf_count) slop at
    /// exhaustion.
    pub(crate) fn scan_budget(&self, spec: BitmapIndexSpec) -> u64 {
        match spec.table_name {
            transaction_bitmap_index::NAME => self.ledger_history.bitmap_bucket_budget_tx(),
            event_bitmap_index::NAME => self.ledger_history.bitmap_bucket_budget_event(),
            other => panic!("unknown bitmap index table {other}; add a budget for it"),
        }
    }
    pub(crate) fn bitmap_skip_policy(&self) -> SkipPolicy {
        self.ledger_history.bitmap_skip_policy()
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
        transaction_filter_to_query(filter, self.ledger_history.max_bitmap_filter_literals())
    }

    pub(crate) fn event_filter_query(&self, filter: &EventFilter) -> Result<BitmapQuery, RpcError> {
        event_filter_to_query(filter, self.ledger_history.max_bitmap_filter_literals())
    }

    /// Callback for `eval_bitmap_query_stream`'s `on_metrics`. Fires exactly
    /// once when the eval pipeline drops and records charged, discarded, and
    /// seek counts for budget and gap-policy tuning.
    pub(crate) fn bitmap_scan_observer(
        &self,
    ) -> impl FnOnce(sui_inverted_index::BitmapScanMetrics) + Send + 'static {
        let metrics = self.metrics.clone();
        let method = self.method;
        move |m| {
            metrics.observe_bitmap_buckets_evaluated(method, m.buckets_evaluated);
            metrics.observe_bitmap_buckets_discarded(method, m.buckets_discarded);
            metrics.observe_bitmap_leaf_seeks(method, m.leaf_seeks);
            tracing::debug!(
                method,
                buckets_evaluated = m.buckets_evaluated,
                buckets_discarded = m.buckets_discarded,
                leaf_seeks = m.leaf_seeks,
                "bitmap scan complete"
            );
        }
    }
}

impl KvRpcServer {
    pub(crate) fn limited_client(&self, operation: &'static str) -> BigTableClient {
        BigTableClient::new(
            self.client.clone(),
            self.request_bigtable_concurrency,
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
            self.ledger_history.clone(),
            self.request_bigtable_concurrency,
            self.stages.clone(),
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
        Ok(tonic::Response::new(
            sui_rpc_api::grpc::deadline::with_deadline(stream, deadline, spec.name),
        ))
    }
}
