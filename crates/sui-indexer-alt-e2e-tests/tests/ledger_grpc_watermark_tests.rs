// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use simulacrum::Simulacrum;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcArgs;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_indexer_alt_reader::ledger_grpc_reader::MAX_BATCH_GET_OBJECTS;
use sui_indexer_alt_reader::ledger_grpc_reader::MAX_BATCH_GET_TRANSACTIONS;
use sui_kv_rpc::KvRpcConfig;
use sui_kvstore::ConcurrentLayer;
use sui_kvstore::PipelineLayer;
use sui_kvstore::SequentialLayer;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;

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
