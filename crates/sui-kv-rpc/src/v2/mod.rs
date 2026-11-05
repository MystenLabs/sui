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
use sui_types::digests::{ChainIdentifier, CheckpointDigest};
use tonic::codegen::BoxStream;

use crate::KvRpcServer;
use crate::admission::AdmittedServiceInfo;
use crate::operation::OperationSpec;
use crate::served_height::ServedHeight;

pub(crate) mod get_checkpoint;
mod get_epoch;
mod get_object;
pub(crate) mod get_transaction;
mod list_checkpoints;
mod list_events;
mod list_transactions;
mod move_package_service;
#[cfg(test)]
mod resume_tests;

#[tonic::async_trait]
impl LedgerService for KvRpcServer {
    async fn get_service_info(
        &self,
        request: tonic::Request<GetServiceInfoRequest>,
    ) -> Result<tonic::Response<GetServiceInfoResponse>, tonic::Status> {
        // Unavailable only before the replica's first served checkpoint, while `/health` reports
        // it not ready.
        let info = AdmittedServiceInfo::of(&request)?
            .ok_or_else(|| tonic::Status::unavailable("served checkpoint not known yet"))?;
        Ok(tonic::Response::new(info.clone()))
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
            ServedHeight::of(&request)?,
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
            ServedHeight::of(&request)?,
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
            ServedHeight::of(&request)?,
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
            ServedHeight::of(&request)?,
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
    Ok(service_info(
        chain_id,
        server_version.as_ref(),
        wm.epoch_hi_inclusive,
        checkpoint_hi_inclusive,
        wm.timestamp_ms_hi_inclusive,
    ))
}

/// The `GetServiceInfo` answer of a replica that serves every checkpoint up to
/// `checkpoint_height`, which belongs to `epoch` and has consensus timestamp `timestamp_ms`.
pub(crate) fn service_info(
    chain_id: ChainIdentifier,
    server_version: Option<&ServerVersion>,
    epoch: u64,
    checkpoint_height: u64,
    timestamp_ms: u64,
) -> GetServiceInfoResponse {
    let mut message = GetServiceInfoResponse::default();
    message.chain_id = Some(CheckpointDigest::new(*chain_id.as_bytes()).base58_encode());
    message.chain = Some(chain_id.chain().as_str().into());
    message.epoch = Some(epoch);
    message.checkpoint_height = Some(checkpoint_height);
    message.timestamp = Some(timestamp_ms_to_proto(timestamp_ms));
    message.lowest_available_checkpoint = Some(0);
    message.lowest_available_checkpoint_objects = Some(0);
    message.server = server_version.map(ToString::to_string);
    message
}

#[cfg(test)]
pub(crate) mod test_utils {
    use std::sync::Arc;

    use prometheus::Registry;
    use prometheus::proto::{Counter, Histogram, MetricFamily};
    use sui_kvstore::BigTableClient as InnerBigTableClient;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_package_resolver::PackageStore;
    use sui_package_resolver::Resolver;
    use sui_rpc::proto::sui::rpc::v2::EventFilter;
    use sui_rpc::proto::sui::rpc::v2::EventLiteral;
    use sui_rpc::proto::sui::rpc::v2::EventTerm;
    use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions;
    use sui_rpc::proto::sui::rpc::v2::SenderFilter;
    use sui_rpc::proto::sui::rpc::v2::TransactionFilter;
    use sui_rpc::proto::sui::rpc::v2::TransactionLiteral;
    use sui_rpc::proto::sui::rpc::v2::TransactionTerm;
    use sui_rpc::proto::sui::rpc::v2::event_literal;
    use sui_rpc::proto::sui::rpc::v2::transaction_literal;
    use sui_types::balance_change::derive_balance_changes_2;
    use sui_types::full_checkpoint_content::Checkpoint;
    use sui_types::storage::{ObjectKey, get_transaction_object_set};
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use crate::KvRpcMetrics;
    use crate::LedgerHistoryConfig;
    use crate::LedgerHistoryMethodConfig;
    use crate::StageConfig;
    use crate::StagesConfig;
    use crate::bigtable_client::BigTableClient;
    use crate::operation::QueryContext;
    use crate::package_store::BigTablePackageStore;
    use crate::served_height::ServedHeight;

    /// A served height above every checkpoint and epoch, for tests of what a handler serves
    /// rather than of where the served height cuts it off.
    pub(crate) const UNBOUNDED_SERVED_HEIGHT: ServedHeight = ServedHeight {
        checkpoint: u64::MAX,
        epoch: u64::MAX,
    };

    pub(crate) fn two_transaction_object_checkpoint() -> Checkpoint {
        TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(10)
            .finish_transaction()
            .start_transaction(0)
            .create_owned_object(11)
            .finish_transaction()
            .build_checkpoint()
    }

    pub(crate) fn kv_transaction_data(
        checkpoint: &Checkpoint,
        index: usize,
    ) -> sui_kvstore::TransactionData {
        let transaction = &checkpoint.transactions[index];
        sui_kvstore::TransactionData {
            digest: transaction.transaction.digest(),
            transaction_data: Some(transaction.transaction.clone()),
            signatures: Some(transaction.signatures.clone()),
            effects: Some(transaction.effects.clone()),
            events: transaction.events.clone(),
            checkpoint_number: checkpoint.summary.sequence_number,
            timestamp: checkpoint.summary.timestamp_ms,
            balance_changes: derive_balance_changes_2(&transaction.effects, &checkpoint.object_set),
            unchanged_loaded_runtime_objects: transaction.unchanged_loaded_runtime_objects.clone(),
        }
    }

    pub(crate) fn canonical_transaction_object_keys(
        checkpoint: &Checkpoint,
        index: usize,
    ) -> Vec<ObjectKey> {
        let transaction = &checkpoint.transactions[index];
        get_transaction_object_set(
            &transaction.transaction,
            &transaction.effects,
            &transaction.unchanged_loaded_runtime_objects,
        )
        .into_iter()
        .collect()
    }

    pub(crate) fn response_object_keys(transaction: &ExecutedTransaction) -> Vec<ObjectKey> {
        let objects = &transaction
            .objects
            .as_ref()
            .expect("objects should be populated when requested")
            .objects;
        assert!(
            !objects.is_empty(),
            "requested object set should be non-empty"
        );
        objects
            .iter()
            .map(|object| {
                ObjectKey(
                    object
                        .object_id
                        .as_deref()
                        .expect("object_id should be populated when requested")
                        .parse()
                        .expect("object_id should be valid"),
                    object
                        .version
                        .expect("version should be populated when requested")
                        .into(),
                )
            })
            .collect()
    }

    pub(crate) fn assert_identity_only_object_mask(transaction: &ExecutedTransaction) {
        let objects = &transaction
            .objects
            .as_ref()
            .expect("objects should be populated when requested")
            .objects;
        assert!(
            !objects.is_empty(),
            "requested object set should be non-empty"
        );
        for object in objects {
            assert!(object.object_id.is_some());
            assert!(object.version.is_some());
            assert!(object.bcs.is_none());
            assert!(object.digest.is_none());
            assert!(object.owner.is_none());
            assert!(object.object_type.is_none());
            assert!(object.has_public_transfer.is_none());
            assert!(object.contents.is_none());
            assert!(object.package.is_none());
            assert!(object.previous_transaction.is_none());
            assert!(object.storage_rebate.is_none());
            assert!(object.json.is_none());
            assert!(object.balance.is_none());
            assert!(object.display.is_none());
        }
    }

    pub(super) const LIST_PIPELINE_METRICS: [&str; 6] = [
        "kv_rpc_response_render_latency_ms",
        "kv_rpc_response_page_bytes",
        "kv_rpc_stream_first_frame_latency_ms",
        "kv_rpc_stream_frame_yield_wait_ms",
        "kv_rpc_stream_watermark_frames_total",
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
    pub(super) fn list_counter<'a>(
        families: &'a [MetricFamily],
        name: &str,
        method: &str,
    ) -> &'a Counter {
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
                .eq([("method", method)]),
            "{name} has unexpected labels"
        );
        metric.get_counter()
    }

    pub(super) fn assert_list_metric_absent(families: &[MetricFamily], name: &str) {
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

    /// A sender predicate whose address does not parse, so no filter holding it compiles.
    fn unparseable_sender() -> SenderFilter {
        let mut sender = SenderFilter::default();
        sender.address = Some("0xnot-an-address".to_owned());
        sender
    }

    /// A transaction filter the server rejects with `InvalidArgument` (see [`unparseable_sender`]).
    pub(super) fn unparseable_transaction_filter() -> TransactionFilter {
        let mut literal = TransactionLiteral::default();
        literal.predicate = Some(transaction_literal::Predicate::Sender(unparseable_sender()));
        let mut term = TransactionTerm::default();
        term.literals = vec![literal];
        let mut filter = TransactionFilter::default();
        filter.terms = vec![term];
        filter
    }

    /// An event filter the server rejects with `InvalidArgument` (see [`unparseable_sender`]).
    pub(super) fn unparseable_event_filter() -> EventFilter {
        let mut literal = EventLiteral::default();
        literal.predicate = Some(event_literal::Predicate::Sender(unparseable_sender()));
        let mut term = EventTerm::default();
        term.literals = vec![literal];
        let mut filter = EventFilter::default();
        filter.terms = vec![term];
        filter
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

    pub(crate) async fn query_context_with_mock_and_registry(
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
            watermark_beacon_buckets: None,
            max_bitmap_filter_literals: Some(1),
            bitmap_schema: Some(sui_kvstore::BitmapSchema::V2),
        };
        let stage = StageConfig {
            max_seeks_per_read: Some(2),
            concurrency: Some(1),
            ..Default::default()
        };
        // Stages whose seeks are buckets also cap their reads at 2 keys, so
        // every stage reads 2 keys at a time whichever buckets the test data
        // uses.
        let bucket_stage = StageConfig {
            max_keys_per_read: Some(2),
            ..stage.clone()
        };
        let stages = StagesConfig {
            tx_seq_digest: Some(bucket_stage.clone()),
            tx_cp_resolve: Some(bucket_stage),
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
                stages,
            ),
            registry,
            mock,
            server,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::TryStreamExt;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_kvstore::testing::insert_checkpoint_rows;
    use sui_rpc::field::FieldMask;
    use sui_rpc::field::FieldMaskUtil;
    use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::KvRpcConfig;
    use crate::LedgerHistoryConfig;
    use crate::StagesConfig;
    use crate::WatermarkSource;

    /// A replica over checkpoints 0 to 2 whose served height moves only when the test publishes
    /// one.
    struct Fixture {
        server: KvRpcServer,
        _mock: MockBigtableServer,
        _mock_server: tokio::task::JoinHandle<()>,
    }

    impl Fixture {
        async fn new() -> Self {
            let mock = MockBigtableServer::new();
            let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
            let mut builder = TestCheckpointBuilder::new(0);
            for object in 10..13 {
                builder = builder
                    .start_transaction(0)
                    .create_owned_object(object)
                    .finish_transaction();
                insert_checkpoint_rows(&mock, &builder.build_checkpoint()).await;
            }

            // A replica that follows indexer streams publishes only from `start_service`, which
            // this never calls, so `publish` alone moves the served height.
            let server = KvRpcServer::new_local_with_config(
                addr.to_string(),
                "test".to_string(),
                None,
                LedgerHistoryConfig::default(),
                KvRpcConfig::default().request_bigtable_concurrency(),
                KvRpcConfig::default().monotonic_read_wait_timeout(),
                StagesConfig::default(),
                WatermarkSource::IndexerStreams(vec![]),
            )
            .await
            .expect("create kv-rpc server");

            Self {
                server,
                _mock: mock,
                _mock_server: mock_server,
            }
        }

        fn publish(&self, checkpoint: u64) {
            let info = service_info(self.server.chain_id, None, 0, checkpoint, checkpoint);
            self.server.cache.send_replace(Some(Arc::new(info)));
        }

        /// `message` admitted at checkpoint 1, with checkpoint 2 published after admission.
        fn admitted_before_publish<T>(&self, message: T) -> tonic::Request<T> {
            self.publish(1);
            let request = self.server.admitted_request(message);
            self.publish(2);
            request
        }
    }

    #[tokio::test]
    async fn service_info_reports_the_admitted_height() {
        let fixture = Fixture::new().await;
        let request = fixture.admitted_before_publish(GetServiceInfoRequest::default());

        let info = fixture
            .server
            .get_service_info(request)
            .await
            .expect("service info")
            .into_inner();
        assert_eq!(info.checkpoint_height, Some(1));
    }

    #[tokio::test]
    async fn latest_checkpoint_is_the_admitted_height() {
        let fixture = Fixture::new().await;
        let mut message = GetCheckpointRequest::default();
        message.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
        let request = fixture.admitted_before_publish(message);

        let checkpoint = fixture
            .server
            .get_checkpoint(request)
            .await
            .expect("latest checkpoint")
            .into_inner()
            .checkpoint
            .expect("response carries a checkpoint");
        assert_eq!(checkpoint.sequence_number, Some(1));
    }

    #[tokio::test]
    async fn checkpoint_list_stops_at_the_admitted_height() {
        let fixture = Fixture::new().await;
        let mut message = ListCheckpointsRequest::default();
        message.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
        let request = fixture.admitted_before_publish(message);

        let frames: Vec<ListCheckpointsResponse> = fixture
            .server
            .list_checkpoints(request)
            .await
            .expect("list checkpoints")
            .into_inner()
            .try_collect()
            .await
            .expect("list checkpoints stream");

        let listed: Vec<u64> = frames
            .iter()
            .filter_map(|frame| frame.checkpoint.as_ref()?.sequence_number)
            .collect();
        assert_eq!(listed, [0, 1]);
        let terminal = frames.last().expect("terminal frame");
        assert_eq!(
            terminal.end.as_ref().map(|end| end.reason()),
            Some(QueryEndReason::LedgerTip)
        );
        assert_eq!(
            terminal.watermark.as_ref().and_then(|w| w.checkpoint),
            Some(1)
        );
    }
}
