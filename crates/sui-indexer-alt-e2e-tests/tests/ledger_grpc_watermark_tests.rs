// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use prometheus::Registry;
use simulacrum::Simulacrum;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcArgs;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_indexer_alt_reader::ledger_grpc_reader::MAX_BATCH_GET_OBJECTS;
use sui_indexer_alt_reader::ledger_grpc_reader::MAX_BATCH_GET_TRANSACTIONS;
use sui_kv_rpc::KvRpcConfig;
use sui_kv_rpc::LedgerHistoryConfig;
use sui_kv_rpc::X_SUI_CONSISTENT_READ_CHECKPOINT;
use sui_kvstore::ALL_PIPELINE_NAMES;
use sui_kvstore::ConcurrentLayer;
use sui_kvstore::PipelineLayer;
use sui_kvstore::SequentialLayer;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoRequest;
use sui_rpc::proto::sui::rpc::v2::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use tonic::transport::Channel;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// A cluster whose list-index BigTable pipelines (`tx_seq_digest`, `transaction_bitmap_index`,
/// `event_bitmap_index`) are throttled to a slow, fixed rate, so the base `checkpoints` pipeline
/// can race ahead of them — reproducing the gap between an unqualified "latest" `GetCheckpoint`
/// (bounded only by the `checkpoints` pipeline) and `GetServiceInfo`'s List-API-aware
/// `checkpoint_height` (bounded by all three).
async fn cluster_with_lagging_list_index_pipelines() -> FullCluster {
    let throttled_concurrent = ConcurrentLayer {
        max_rows_per_second: Some(1),
        ..Default::default()
    };
    let throttled_sequential = SequentialLayer {
        max_rows_per_second: Some(1),
        ..Default::default()
    };

    FullCluster::new_with_configs(
        Simulacrum::new(),
        OffchainClusterConfig {
            kv_rpc_config: KvRpcConfig {
                enable_list_apis: Some(true),
                ..Default::default()
            },
            bt_pipeline_layer: PipelineLayer {
                tx_seq_digest: throttled_concurrent,
                transaction_bitmap_index: throttled_sequential.clone(),
                event_bitmap_index: throttled_sequential,
                ..Default::default()
            },
            ..Default::default()
        },
        &Registry::new(),
    )
    .await
    .expect("Failed to create cluster")
}

/// Execute a `transfer_sui(sender, None)` self-transfer and return the updated gas reference.
async fn transfer_self(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &AccountKeyPair,
    gas: ObjectRef,
) -> ObjectRef {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, None);
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("transfer failed");
    assert!(err.is_none(), "transfer failed: {err:?}");
    fx.mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas mutated")
}

/// `LedgerGrpcReader::checkpoint_watermark()` must resolve the checkpoint via `GetServiceInfo`'s
/// `checkpoint_height`, which is bounded by the list-index pipelines, not an unqualified "latest"
/// `GetCheckpoint`, which is only bounded by the base `checkpoints` pipeline. With the list-index
/// pipelines throttled well behind, the two diverge — reproducing the race a caller reading the
/// unqualified "latest" would hit against a real, indexing-in-progress deployment.
#[tokio::test]
async fn checkpoint_watermark_tracks_list_api_lag() {
    let mut cluster = cluster_with_lagging_list_index_pipelines().await;
    let (sender, kp, mut gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // Warm up: let the first checkpoint fully sync, including the throttled list-index
    // pipelines, so `GetServiceInfo` has an initialized baseline watermark to report.
    gas = transfer_self(&mut cluster, sender, &kp, gas).await;
    cluster.create_checkpoint().await;

    // Now race ahead: create several more checkpoints, each time waiting only for the base
    // `checkpoints` pipeline (not the throttled list-index pipelines) to catch up, so the two
    // fall out of sync.
    let mut latest_base_checkpoint = 0;
    for _ in 0..5 {
        gas = transfer_self(&mut cluster, sender, &kp, gas).await;
        latest_base_checkpoint = cluster
            .create_checkpoint_before_list_apis_sync()
            .await
            .sequence_number;
    }

    let mut raw_client = LedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .expect("connect to kv-rpc");
    let list_api_checkpoint_height = raw_client
        .get_service_info(GetServiceInfoRequest::default())
        .await
        .expect("GetServiceInfo")
        .into_inner()
        .checkpoint_height
        .expect("checkpoint_height present");

    assert!(
        list_api_checkpoint_height < latest_base_checkpoint,
        "test setup didn't reproduce a lag: list-index pipelines (height {list_api_checkpoint_height}) \
         caught up to the base pipeline (height {latest_base_checkpoint})",
    );

    let reader = LedgerGrpcReader::new(
        cluster.kv_rpc_url().to_string().parse().unwrap(),
        LedgerGrpcArgs::default(),
        None,
        &Registry::new(),
        MAX_BATCH_GET_TRANSACTIONS,
        MAX_BATCH_GET_OBJECTS,
    )
    .await
    .expect("construct LedgerGrpcReader");

    let watermark = reader
        .checkpoint_watermark()
        .await
        .expect("checkpoint_watermark should succeed");

    assert_eq!(
        watermark.sequence_number, list_api_checkpoint_height,
        "checkpoint_watermark() must track GetServiceInfo's List-API-aware checkpoint_height, \
         not the base checkpoint pipeline's (unbounded by list-index lag) latest checkpoint",
    );
}

/// A cluster serving the List APIs at full pipeline speed, a client, and the highest checkpoint
/// kv-rpc can serve.
///
/// `consistent_read_wait` overrides how long kv-rpc holds a request whose checkpoint it has not
/// reached; `None` leaves the server's own default in place.
///
/// These tests do not need the throttled fixture above: the consistent-read wait is about a
/// checkpoint the replica has not reached, which any checkpoint past the tip supplies.
async fn cluster_at_tip(
    consistent_read_wait: Option<Duration>,
) -> (FullCluster, LedgerServiceClient<Channel>, u64) {
    let mut cluster = FullCluster::new_with_configs(
        Simulacrum::new(),
        OffchainClusterConfig {
            kv_rpc_config: KvRpcConfig {
                enable_list_apis: Some(true),
                ledger_history: Some(LedgerHistoryConfig {
                    consistent_read_wait_timeout_ms: consistent_read_wait
                        .map(|wait| wait.as_millis() as u64),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
        &Registry::new(),
    )
    .await
    .expect("Failed to create cluster");
    // kv-rpc serves the lowest of its per-pipeline watermarks, so every pipeline has to reach this
    // checkpoint before it counts as the tip. `create_checkpoint` would additionally wait on the
    // indexer, consistent store and GraphQL, which no assertion here reads.
    let tip = cluster
        .create_checkpoint_before_list_apis_sync()
        .await
        .sequence_number;
    cluster
        .wait_for_bigtable(&ALL_PIPELINE_NAMES, tip, Duration::from_secs(60))
        .await
        .expect("Timed out waiting for BigTable pipelines");

    let client = LedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .expect("connect to kv-rpc");

    (cluster, client, tip)
}

fn request_at<T>(payload: T, checkpoint: u64) -> tonic::Request<T> {
    let mut request = tonic::Request::new(payload);
    request
        .metadata_mut()
        .insert(X_SUI_CONSISTENT_READ_CHECKPOINT, checkpoint.into());
    request
}

/// Asking for a checkpoint this replica has not indexed must be refused as retryable. Serving it
/// from the local watermark instead would quietly narrow the range and report the scan as having
/// reached the ledger tip, which is how a load-balanced client sees the tip move backwards.
#[tokio::test]
async fn consistent_read_beyond_local_watermark_is_retryable() {
    let (_cluster, mut client, tip) = cluster_at_tip(None).await;
    let unreachable = tip + 1_000;

    let err = client
        .list_transactions(request_at(ListTransactionsRequest::default(), unreachable))
        .await
        .expect_err("replica cannot serve this checkpoint");
    assert_eq!(err.code(), tonic::Code::Unavailable);

    let err = client
        .get_service_info(request_at(GetServiceInfoRequest::default(), unreachable))
        .await
        .expect_err("replica cannot serve this checkpoint");
    assert_eq!(err.code(), tonic::Code::Unavailable);
}

/// A checkpoint this replica has already reached is served without waiting.
#[tokio::test]
async fn consistent_read_within_local_watermark_is_served() {
    let (_cluster, mut client, tip) = cluster_at_tip(None).await;

    let mut stream = client
        .list_transactions(request_at(ListTransactionsRequest::default(), tip))
        .await
        .expect("replica can serve this checkpoint")
        .into_inner();
    assert!(
        stream.message().await.expect("stream frame").is_some(),
        "expected at least one frame",
    );

    let info = client
        .get_service_info(request_at(GetServiceInfoRequest::default(), tip))
        .await
        .expect("replica can serve this checkpoint")
        .into_inner();
    assert!(info.checkpoint_height.unwrap() >= tip);
}

/// A malformed header is a client bug, not a reason to wait.
#[tokio::test]
async fn consistent_read_header_must_be_a_checkpoint() {
    let (_cluster, mut client, _) = cluster_at_tip(None).await;

    let mut request = tonic::Request::new(ListTransactionsRequest::default());
    request.metadata_mut().insert(
        X_SUI_CONSISTENT_READ_CHECKPOINT,
        "not-a-checkpoint".parse().unwrap(),
    );

    let err = client
        .list_transactions(request)
        .await
        .expect_err("malformed header should be rejected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

/// The point of the wait: a request for a checkpoint this replica has not reached is held rather
/// than refused, and is served once the watermark catches up.
#[tokio::test]
async fn consistent_read_waits_for_the_watermark_to_catch_up() {
    // Indexing a checkpoint takes far longer than the default wait, which is sized to shed requests
    // to a replica that is genuinely behind rather than to outlast indexing.
    let (mut cluster, mut client, tip) = cluster_at_tip(Some(Duration::from_secs(60))).await;
    let next = tip + 1;

    // `join!` polls in order, so the request is in flight and waiting before the checkpoint that
    // satisfies it exists. Answering from the height held at that point would fail the assertion
    // below, so only a request that actually waited can pass.
    let (served, _) = tokio::join!(
        client.get_service_info(request_at(GetServiceInfoRequest::default(), next)),
        cluster.create_checkpoint_before_list_apis_sync(),
    );

    let info = served
        .expect("served once the watermark advanced")
        .into_inner();
    assert!(info.checkpoint_height.unwrap() >= next);
}
