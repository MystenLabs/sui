// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::KvRpcServer;
use crate::operation::OperationSpec;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_server::LedgerService;
use tonic::codegen::BoxStream;

mod list_checkpoints;
mod list_events;
mod list_transactions;

// Per-RPC hard request timeout (from `LedgerHistoryConfig`). The outer
// `operation::with_deadline` wrapper drops the response stream with
// `DeadlineExceeded` when this fires; debounced intermediate `Watermark` frames
// let the client resume from wherever it got to.
#[tonic::async_trait]
impl LedgerService for KvRpcServer {
    async fn list_checkpoints(
        &self,
        request: tonic::Request<ListCheckpointsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListCheckpointsResponse>>, tonic::Status> {
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
        self.serve_query_stream(
            OperationSpec::new("list_events", self.ledger_history.list_events().timeout),
            request,
            list_events::list_events,
        )
        .await
    }
}

#[cfg(test)]
mod test_utils {
    use std::sync::Arc;

    use prometheus::Registry;
    use sui_kvstore::BigTableClient as InnerBigTableClient;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_package_resolver::PackageStore;
    use sui_package_resolver::Resolver;
    use sui_rpc::proto::sui::rpc::v2alpha::Ordering;
    use sui_rpc::proto::sui::rpc::v2alpha::QueryOptions;

    use crate::KvRpcMetrics;
    use crate::LedgerHistoryConfig;
    use crate::LedgerHistoryMethodConfig;
    use crate::StageConfig;
    use crate::StagesConfig;
    use crate::bigtable_client::BigTableClient;
    use crate::operation::QueryContext;
    use crate::package_store::BigTablePackageStore;

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
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let inner = InnerBigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");

        let metrics = KvRpcMetrics::new(&Registry::new());
        let client =
            BigTableClient::new(inner.clone(), 2, metrics.bigtable_limiter.clone(), method);
        let package_store: Arc<dyn PackageStore> = Arc::new(BigTablePackageStore::new(inner));
        let package_resolver = Arc::new(Resolver::new(package_store));

        let method_config = LedgerHistoryMethodConfig {
            timeout_ms: Some(5_000),
            default_limit_items: Some(10),
            max_limit_items: Some(100),
        };
        let ledger_history = LedgerHistoryConfig {
            list_transactions: Some(method_config.clone()),
            list_events: Some(method_config.clone()),
            list_checkpoints: Some(method_config),
            bitmap_bucket_budget_tx: Some(10),
            bitmap_bucket_budget_event: Some(10),
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
            server,
        )
    }
}
