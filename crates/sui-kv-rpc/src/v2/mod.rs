// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetObjectsRequest, BatchGetObjectsResponse, BatchGetTransactionsRequest,
    BatchGetTransactionsResponse, GetCheckpointRequest, GetCheckpointResponse, GetEpochRequest,
    GetEpochResponse, GetObjectRequest, GetObjectResponse, GetServiceInfoRequest,
    GetServiceInfoResponse, GetTransactionRequest, GetTransactionResponse, ListCheckpointsRequest,
    ListCheckpointsResponse, ListEventsRequest, ListEventsResponse, ListTransactionsRequest,
    ListTransactionsResponse, ledger_service_server::LedgerService,
};
use sui_rpc_api::proto::timestamp_ms_to_proto;
use sui_rpc_api::{CheckpointNotFoundError, RpcError, ServerVersion};
use sui_sdk_types::Digest;
use sui_types::digests::ChainIdentifier;
use tonic::codegen::BoxStream;

use crate::KvRpcServer;
use crate::operation::OperationSpec;

pub(crate) mod get_checkpoint;
mod get_epoch;
mod get_object;
pub(crate) mod get_transaction;
mod list_checkpoints;
mod list_events;
mod list_transactions;

#[tonic::async_trait]
impl LedgerService for KvRpcServer {
    async fn get_service_info(
        &self,
        _: tonic::Request<GetServiceInfoRequest>,
    ) -> Result<tonic::Response<GetServiceInfoResponse>, tonic::Status> {
        {
            let cache = self.cache.read().await;
            if let Some(cached_info) = cache.as_ref() {
                return Ok(tonic::Response::new(cached_info.clone()));
            }
        }
        // If no cache available, fetch directly and update cache
        get_service_info(
            self.client.clone(),
            self.chain_id,
            self.server_version.clone(),
            &self.service_info_watermark_pipelines,
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn get_object(
        &self,
        request: tonic::Request<GetObjectRequest>,
    ) -> Result<tonic::Response<GetObjectResponse>, tonic::Status> {
        get_object::get_object(
            self.client.clone(),
            request.into_inner(),
            &self.package_resolver,
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn batch_get_objects(
        &self,
        request: tonic::Request<BatchGetObjectsRequest>,
    ) -> Result<tonic::Response<BatchGetObjectsResponse>, tonic::Status> {
        get_object::batch_get_objects(
            self.client.clone(),
            request.into_inner(),
            &self.package_resolver,
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn get_transaction(
        &self,
        request: tonic::Request<GetTransactionRequest>,
    ) -> Result<tonic::Response<GetTransactionResponse>, tonic::Status> {
        get_transaction::get_transaction(
            self.limited_client("GetTransaction"),
            &self.stages,
            request.into_inner(),
            &self.package_resolver,
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn batch_get_transactions(
        &self,
        request: tonic::Request<BatchGetTransactionsRequest>,
    ) -> Result<tonic::Response<BatchGetTransactionsResponse>, tonic::Status> {
        get_transaction::batch_get_transactions(
            self.limited_client("BatchGetTransactions"),
            &self.stages,
            request.into_inner(),
            &self.package_resolver,
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn get_checkpoint(
        &self,
        request: tonic::Request<GetCheckpointRequest>,
    ) -> Result<tonic::Response<GetCheckpointResponse>, tonic::Status> {
        get_checkpoint::get_checkpoint(
            self.client.clone(),
            self.limited_client("GetCheckpoint"),
            &self.stages,
            request.into_inner(),
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn get_epoch(
        &self,
        request: tonic::Request<GetEpochRequest>,
    ) -> Result<tonic::Response<GetEpochResponse>, tonic::Status> {
        get_epoch::get_epoch(
            self.client.clone(),
            request.into_inner(),
            self.chain_id.chain(),
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    // The list RPC hard timeout covers both computation and response delivery.
    // Expiry drops the stream with `DeadlineExceeded` without emitting a
    // terminal resume-cursor frame, so a client can resume only from the last
    // `Watermark` it retained.
    async fn list_checkpoints(
        &self,
        request: tonic::Request<ListCheckpointsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListCheckpointsResponse>>, tonic::Status> {
        self.check_query_apis_enabled()?;
        self.serve_query_stream(
            OperationSpec::new(
                "list_checkpoints",
                self.ledger_history.list_checkpoints().timeout,
            ),
            request,
            list_checkpoints::list_checkpoints,
        )
        .await
    }

    async fn list_transactions(
        &self,
        request: tonic::Request<ListTransactionsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListTransactionsResponse>>, tonic::Status> {
        self.check_query_apis_enabled()?;
        self.serve_query_stream(
            OperationSpec::new(
                "list_transactions",
                self.ledger_history.list_transactions().timeout,
            ),
            request,
            list_transactions::list_transactions,
        )
        .await
    }

    async fn list_events(
        &self,
        request: tonic::Request<ListEventsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListEventsResponse>>, tonic::Status> {
        self.check_query_apis_enabled()?;
        self.serve_query_stream(
            OperationSpec::new("list_events", self.ledger_history.list_events().timeout),
            request,
            list_events::list_events,
        )
        .await
    }
}

pub(crate) async fn get_service_info(
    mut client: BigTableClient,
    chain_id: ChainIdentifier,
    server_version: Option<ServerVersion>,
    watermark_pipelines: &[&str],
) -> Result<GetServiceInfoResponse, RpcError> {
    let Some(wm) = client
        .get_watermark_for_pipelines(watermark_pipelines)
        .await?
    else {
        return Err(CheckpointNotFoundError::sequence_number(0).into());
    };
    let Some(checkpoint_hi_inclusive) = wm.checkpoint_hi_inclusive else {
        return Err(CheckpointNotFoundError::sequence_number(0).into());
    };
    let mut message = GetServiceInfoResponse::default();
    message.chain_id = Some(Digest::new(chain_id.as_bytes().to_owned()).to_string());
    message.chain = Some(chain_id.chain().as_str().into());
    message.epoch = Some(wm.epoch_hi_inclusive);
    message.checkpoint_height = Some(checkpoint_hi_inclusive);
    message.timestamp = Some(timestamp_ms_to_proto(wm.timestamp_ms_hi_inclusive));
    message.lowest_available_checkpoint = Some(0);
    message.lowest_available_checkpoint_objects = Some(0);
    message.server = server_version.as_ref().map(ToString::to_string);
    Ok(message)
}

#[cfg(test)]
mod test_utils {
    use std::sync::Arc;

    use prometheus::Registry;
    use prometheus::proto::{Histogram, MetricFamily};
    use sui_kvstore::BigTableClient as InnerBigTableClient;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_package_resolver::PackageStore;
    use sui_package_resolver::Resolver;
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions;

    use crate::KvRpcMetrics;
    use crate::LedgerHistoryConfig;
    use crate::LedgerHistoryMethodConfig;
    use crate::StageConfig;
    use crate::StagesConfig;
    use crate::bigtable_client::BigTableClient;
    use crate::operation::QueryContext;
    use crate::package_store::BigTablePackageStore;

    pub(super) const LIST_PIPELINE_METRICS: [&str; 5] = [
        "kv_rpc_response_render_latency_ms",
        "kv_rpc_response_page_bytes",
        "kv_rpc_stream_first_item_latency_ms",
        "kv_rpc_stream_item_yield_wait_ms",
        "kv_rpc_final_stream_poll_wait_ms",
    ];

    pub(super) fn list_histogram<'a>(
        families: &'a [MetricFamily],
        name: &str,
        method: &str,
        resolution: &str,
    ) -> &'a Histogram {
        let family = families
            .iter()
            .find(|family| family.name() == name)
            .unwrap_or_else(|| panic!("metric family {name} not registered"));
        let [metric] = family.get_metric() else {
            panic!("{name} has unexpected series");
        };
        assert!(
            metric
                .get_label()
                .iter()
                .map(|label| (label.name(), label.value()))
                .eq([("method", method), ("resolution", resolution)]),
            "{name} has unexpected labels"
        );
        metric.get_histogram()
    }

    pub(super) fn assert_list_histogram_absent(families: &[MetricFamily], name: &str) {
        assert!(
            families.iter().all(|family| family.name() != name),
            "{name} unexpectedly registered"
        );
    }

    pub(super) fn ascending_options() -> QueryOptions {
        let mut options = QueryOptions::default();
        options.limit = Some(10);
        options.ordering = Some(Ordering::Ascending as i32);
        options
    }

    pub(super) async fn query_context(
        method: &'static str,
        checkpoint_hi_exclusive: u64,
    ) -> (QueryContext, tokio::task::JoinHandle<()>) {
        let (ctx, _registry, server) =
            query_context_with_registry(method, checkpoint_hi_exclusive).await;
        (ctx, server)
    }

    pub(super) async fn query_context_with_registry(
        method: &'static str,
        checkpoint_hi_exclusive: u64,
    ) -> (QueryContext, Registry, tokio::task::JoinHandle<()>) {
        let (ctx, registry, _mock, server) =
            query_context_with_mock_and_registry(method, checkpoint_hi_exclusive).await;
        (ctx, registry, server)
    }

    pub(super) async fn query_context_with_mock_and_registry(
        method: &'static str,
        checkpoint_hi_exclusive: u64,
    ) -> (
        QueryContext,
        Registry,
        MockBigtableServer,
        tokio::task::JoinHandle<()>,
    ) {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let inner = InnerBigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");

        let registry = Registry::new();
        let metrics = KvRpcMetrics::new(&registry);
        let client =
            BigTableClient::new(inner.clone(), 2, metrics.bigtable_limiter.clone(), method);
        let package_store: Arc<dyn PackageStore> = Arc::new(BigTablePackageStore::new(inner));
        let package_resolver = Arc::new(Resolver::new(package_store));

        let method_config = LedgerHistoryMethodConfig {
            timeout_ms: Some(5_000),
            default_limit_items: Some(10),
            max_limit_items: Some(100),
            render_ahead: Some(2),
        };
        let ledger_history = LedgerHistoryConfig {
            list_transactions: Some(method_config.clone()),
            list_events: Some(method_config.clone()),
            list_checkpoints: Some(method_config),
            bitmap_bucket_budget_tx: Some(10),
            bitmap_bucket_budget_event: Some(10),
            bitmap_drain_probe_rows: None,
            max_bitmap_filter_literals: Some(1),
        };
        let stage = StageConfig {
            chunk_size: Some(2),
            concurrency: Some(1),
        };
        let stages = StagesConfig {
            tx_seq_digest: Some(stage.clone()),
            transactions: Some(stage.clone()),
            objects: Some(stage.clone()),
            checkpoints: Some(stage),
        };

        (
            QueryContext::new(
                client,
                package_resolver,
                metrics,
                method,
                checkpoint_hi_exclusive,
                ledger_history,
                2,
                stages,
            ),
            registry,
            mock,
            server,
        )
    }
}
