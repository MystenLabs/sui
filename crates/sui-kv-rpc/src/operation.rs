// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::time::Duration;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::SkipPolicy;
use sui_inverted_index::WatermarkPacing;
use sui_kvstore::BitmapIndexSpec;
use sui_rpc::proto::sui::rpc::v2::EventFilter;
use sui_rpc::proto::sui::rpc::v2::TransactionFilter;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::query_options::RangeExhaustion;
use tokio::time::Instant;

use crate::KvRpcMetrics;
use crate::KvRpcServer;
use crate::PackageResolver;
use crate::bigtable_client::BigTableClient;
use crate::config::{LedgerHistoryConfig, PipelineStage, ResolvedStageConfig, StagesConfig};
use crate::served_height::ServedHeight;
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
        stages: StagesConfig,
    ) -> Self {
        Self {
            client,
            package_resolver,
            metrics,
            method,
            checkpoint_hi_exclusive,
            ledger_history,
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

    /// The transaction bitmap index table the List APIs read.
    pub(crate) fn tx_bitmap_spec(&self) -> BitmapIndexSpec {
        BitmapIndexSpec::tx(self.ledger_history.bitmap_schema())
    }

    /// The event bitmap index table the List APIs read.
    pub(crate) fn event_bitmap_spec(&self) -> BitmapIndexSpec {
        BitmapIndexSpec::event(self.ledger_history.bitmap_schema())
    }

    pub(crate) fn bitmap_skip_policy(&self) -> SkipPolicy {
        self.ledger_history.bitmap_skip_policy()
    }

    /// When the bitmap-query evaluator emits the scan watermarks that
    /// [`crate::pipeline::resolve_scan_watermarks`] turns into progress
    /// beacons, one BigTable lookup each: the first at once, later ones after
    /// `watermark-beacon-buckets` of scan work (bucket rows charged, with each
    /// reopen step counted as `bitmap-drain-probe-rows`) without an item or
    /// beacon.
    ///
    /// Only a range that ends at a cursor bound keeps the held watermark at the
    /// end of the scan. Its terminal frame claims exactly the coverage of the
    /// frames before it (`ScanTerminal::into_watermark` claims
    /// `covered_checkpoint_bound` for `CursorBound`), so the scan's last
    /// progress must reach the resolver for the terminal frame to claim it. A
    /// range that ends naturally claims the range's last checkpoint itself, so
    /// resolving the held watermark there would be a wasted lookup.
    pub(crate) fn watermark_pacing(&self, exhaustion: RangeExhaustion) -> WatermarkPacing {
        WatermarkPacing {
            min_buckets: self.ledger_history.watermark_beacon_buckets().get(),
            emit_held_at_end: matches!(exhaustion, RangeExhaustion::CursorBound { .. }),
        }
    }

    pub(crate) fn observe_response_render(
        &self,
        resolution: &'static str,
        elapsed: std::time::Duration,
    ) {
        self.metrics
            .observe_response_render(self.method, resolution, elapsed);
    }

    pub(crate) fn observe_response_page_bytes(&self, resolution: &'static str, bytes: usize) {
        self.metrics
            .observe_response_page_bytes(self.method, resolution, bytes);
    }

    pub(crate) fn observe_stream_first_frame_latency(
        &self,
        resolution: &'static str,
        elapsed: std::time::Duration,
    ) {
        self.metrics
            .observe_stream_first_frame_latency(self.method, resolution, elapsed);
    }

    pub(crate) fn observe_stream_frame_yield_wait(
        &self,
        resolution: &'static str,
        elapsed: std::time::Duration,
    ) {
        self.metrics
            .observe_stream_frame_yield_wait(self.method, resolution, elapsed);
    }

    pub(crate) fn inc_stream_watermark_frames(&self) {
        self.metrics.inc_stream_watermark_frames(self.method);
    }

    pub(crate) async fn next_response_item<T>(
        &self,
        resolution: &'static str,
        stream: &mut BoxStream<'static, T>,
    ) -> Option<T> {
        let poll_started_at = Instant::now();
        let item = stream.next().await;
        self.metrics.observe_final_stream_poll_wait(
            self.method,
            resolution,
            poll_started_at.elapsed(),
        );
        item
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

    /// The context of an `operation` request served at `served_height`, reading through
    /// `client`, the request's gated client (see [`Self::limited_client`]).
    pub(crate) fn query_context(
        &self,
        operation: &'static str,
        client: BigTableClient,
        served_height: ServedHeight,
    ) -> Result<QueryContext, RpcError> {
        let checkpoint_hi_exclusive = served_height.checkpoint.checked_add(1).ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                "served checkpoint height cannot be represented as an exclusive bound",
            )
        })?;
        Ok(QueryContext::new(
            client,
            self.package_resolver.clone(),
            self.metrics.clone(),
            operation,
            checkpoint_hi_exclusive,
            self.ledger_history.clone(),
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
        let ctx = self.query_context(
            spec.name,
            self.limited_client(spec.name),
            ServedHeight::of(&request)?,
        )?;
        self.serve_stream(spec, handler(ctx, request.into_inner()))
            .await
    }

    /// Serve the stream `stream` builds, under `spec`'s deadline, which covers both building it
    /// and delivering it.
    pub(crate) async fn serve_stream<T, Fut>(
        &self,
        spec: OperationSpec,
        stream: Fut,
    ) -> Result<tonic::Response<BoxStream<'static, Result<T, tonic::Status>>>, tonic::Status>
    where
        T: Send + 'static,
        Fut: Future<Output = Result<BoxStream<'static, Result<T, RpcError>>, RpcError>> + Send,
    {
        let deadline = Instant::now() + spec.timeout;
        let stream = tokio::time::timeout_at(deadline, stream)
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
