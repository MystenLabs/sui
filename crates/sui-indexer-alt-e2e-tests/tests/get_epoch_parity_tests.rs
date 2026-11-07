// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `LedgerService/GetEpoch` on a fullnode and on kv-rpc indexing the same chain.

use std::time::Duration;

use prometheus::Registry;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Epoch;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use tempfile::TempDir;
use test_cluster::TestClusterBuilder;
use tonic::transport::Channel;

/// Every `Epoch` field both servers read from their stored epoch rows.
const STORED_FIELDS: &[&str] = &[
    "epoch",
    "first_checkpoint",
    "last_checkpoint",
    "start",
    "end",
    "reference_gas_price",
    "protocol_config.protocol_version",
    "committee",
    "transaction_start_inclusive",
    "transaction_end_exclusive",
    "ended_in_safe_mode",
    "system_epoch_info",
    "epoch_commitments",
];

async fn get_epoch(client: &mut LedgerServiceClient<Channel>, epoch: u64, paths: &[&str]) -> Epoch {
    let request =
        GetEpochRequest::new(epoch).with_read_mask(FieldMask::from_paths(paths.iter().copied()));
    client
        .get_epoch(request)
        .await
        .unwrap_or_else(|status| panic!("GetEpoch {epoch} {paths:?}: {status:?}"))
        .into_inner()
        .epoch
        .unwrap_or_else(|| panic!("GetEpoch {epoch} {paths:?} found no epoch"))
}

async fn latest_checkpoint(client: &mut LedgerServiceClient<Channel>) -> u64 {
    client
        .get_checkpoint(
            GetCheckpointRequest::latest()
                .with_read_mask(FieldMask::from_paths(["sequence_number"])),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap()
        .sequence_number
        .unwrap()
}

/// A fullnode and kv-rpc serve the same stored and end-of-epoch fields for an ended and a running
/// epoch, the same system state for an ended epoch, and the same `NOT_FOUND` for an epoch that has
/// not started.
#[tokio::test]
async fn test_get_epoch_matches_between_fullnode_and_kv_rpc() {
    let temp_dir = TempDir::new().unwrap();
    let ingestion_dir = temp_dir.path().to_path_buf();
    let onchain = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_data_ingestion_dir(ingestion_dir.clone())
        .with_epoch_duration_ms(300_000)
        .disable_fullnode_pruning()
        .build()
        .await;
    let offchain = OffchainCluster::new(
        ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(ingestion_dir),
                ..Default::default()
            },
            ..Default::default()
        },
        OffchainClusterConfig::default(),
        &Registry::new(),
    )
    .await
    .unwrap();

    onchain.trigger_reconfiguration().await;
    onchain.wait_for_rpc_index_ready().await;
    let mut fullnode = LedgerServiceClient::connect(onchain.rpc_url().to_owned())
        .await
        .unwrap();
    let mut kv_rpc = LedgerServiceClient::connect(offchain.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Both servers must serve a checkpoint of epoch 1, or it does not exist for them yet.
    let last_of_epoch_0 = get_epoch(&mut fullnode, 0, &["last_checkpoint"])
        .await
        .last_checkpoint
        .unwrap();
    let first_of_epoch_1 = last_of_epoch_0 + 1;
    tokio::time::timeout(Duration::from_secs(60), async {
        while latest_checkpoint(&mut fullnode).await < first_of_epoch_1 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("epoch 1 has a checkpoint");
    onchain.wait_for_rpc_index_ready().await;
    offchain
        .wait_for_kv_rpc(first_of_epoch_1, Duration::from_secs(60))
        .await
        .expect("kv-rpc catches up to the fullnode");

    for epoch in [0, 1] {
        let served = get_epoch(&mut fullnode, epoch, STORED_FIELDS).await;
        if epoch == 0 {
            // The comparison only means something if the ended epoch's end fields are set.
            assert!(served.transaction_end_exclusive.is_some());
            assert!(served.system_epoch_info.is_some());
        }
        assert_eq!(
            get_epoch(&mut kv_rpc, epoch, STORED_FIELDS).await,
            served,
            "epoch {epoch}"
        );
    }
    let ended_state = get_epoch(&mut fullnode, 0, &["system_state"]).await;
    assert_eq!(ended_state.system_state.as_ref().unwrap().epoch, Some(0));
    assert_eq!(
        get_epoch(&mut kv_rpc, 0, &["system_state"]).await,
        ended_state
    );

    let request = || GetEpochRequest::new(2).with_read_mask(FieldMask::from_paths(["epoch"]));
    let fullnode_status = fullnode.get_epoch(request()).await.unwrap_err();
    assert_eq!(fullnode_status.code(), tonic::Code::NotFound);
    assert_eq!(fullnode_status.message(), "Epoch 2 not found");
    let kv_rpc_status = kv_rpc.get_epoch(request()).await.unwrap_err();
    assert_eq!(kv_rpc_status.code(), fullnode_status.code());
    assert_eq!(kv_rpc_status.message(), fullnode_status.message());
}
