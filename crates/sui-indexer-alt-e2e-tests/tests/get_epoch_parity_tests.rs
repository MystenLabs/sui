// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `LedgerService/GetEpoch` on a fullnode and on kv-rpc indexing the same chain.

use std::path::Path;
use std::time::Duration;

use prometheus::Registry;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_kv_rpc::KvRpcConfig;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Epoch;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_test_transaction_builder::make_staking_transaction;
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

/// What a client polling the latest epoch's state reads. kv-rpc derives the running epoch's
/// `committee` from the `system_state` it reads when both are asked for.
const LATEST_STATE_FIELDS: &[&str] = &["epoch", "last_checkpoint", "committee", "system_state"];

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

async fn get_latest_epoch(client: &mut LedgerServiceClient<Channel>, paths: &[&str]) -> Epoch {
    let request =
        GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(paths.iter().copied()));
    client
        .get_epoch(request)
        .await
        .unwrap_or_else(|status| panic!("GetEpoch latest {paths:?}: {status:?}"))
        .into_inner()
        .epoch
        .unwrap_or_else(|| panic!("GetEpoch latest {paths:?} found no epoch"))
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

/// Copy the fullnode's checkpoint files `*next..=up_to` into kv-rpc's ingestion directory, so
/// kv-rpc's served height stops at `up_to`. Each file is written under a temporary name and then
/// renamed, so the indexer never reads a partial file.
fn release_checkpoints(fullnode_dir: &Path, kv_rpc_dir: &Path, next: &mut u64, up_to: u64) {
    while *next <= up_to {
        let name = format!("{next}.binpb.zst");
        let partial = kv_rpc_dir.join(format!("{name}.partial"));
        std::fs::copy(fullnode_dir.join(&name), &partial)
            .unwrap_or_else(|e| panic!("copy checkpoint {next}: {e}"));
        std::fs::rename(&partial, kv_rpc_dir.join(&name)).unwrap();
        *next += 1;
    }
}

/// A fullnode and kv-rpc serve the same stored and end-of-epoch fields and the same system state
/// for an ended epoch, and the same `NOT_FOUND` for an epoch that has not started. A running epoch
/// is not compared: the fullnode answers for it from its live state and kv-rpc at its served
/// height, so the two answers align only on a stopped chain once both servers reach the same
/// height.
///
/// Around the end of epoch 0 at its last checkpoint L, kv-rpc's latest epoch is epoch 0 at its
/// state at L - 1, both while kv-rpc serves L - 1 and once it serves L, since the change-epoch
/// transaction in L has already written epoch 1's state. kv-rpc is held at L - 1, L and then
/// epoch 1's first checkpoint by releasing checkpoints to it one step at a time.
#[tokio::test]
async fn test_get_epoch_matches_between_fullnode_and_kv_rpc() {
    let fullnode_dir = TempDir::new().unwrap();
    let kv_rpc_dir = TempDir::new().unwrap();
    let onchain = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_data_ingestion_dir(fullnode_dir.path().to_path_buf())
        .with_epoch_duration_ms(300_000)
        .disable_fullnode_pruning()
        .build()
        .await;
    onchain.wait_for_rpc_index_ready().await;
    let mut fullnode = LedgerServiceClient::connect(onchain.rpc_url().to_owned())
        .await
        .unwrap();

    let mut next_to_release = 0;
    let genesis_tip = latest_checkpoint(&mut fullnode).await;
    release_checkpoints(
        fullnode_dir.path(),
        kv_rpc_dir.path(),
        &mut next_to_release,
        genesis_tip,
    );
    let offchain = OffchainCluster::new(
        ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(kv_rpc_dir.path().to_path_buf()),
                ..Default::default()
            },
            ..Default::default()
        },
        OffchainClusterConfig {
            kv_rpc_config: KvRpcConfig {
                enable_object_at_checkpoint: Some(true),
                ..Default::default()
            },
            ..Default::default()
        },
        &Registry::new(),
    )
    .await
    .unwrap();
    let mut kv_rpc = LedgerServiceClient::connect(offchain.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Stake mid-epoch, so epoch 0's state at its end differs from its start snapshot.
    let validator = onchain.swarm.config().validator_configs()[0].sui_address();
    let stake = make_staking_transaction(&onchain.wallet, validator).await;
    onchain.wallet.execute_transaction_must_succeed(stake).await;

    onchain.trigger_reconfiguration().await;
    // Epoch 0's end is indexed from its last checkpoint, which the fullnode executed, and wrote to
    // its ingestion directory, before it entered epoch 1.
    onchain.wait_for_rpc_index_ready().await;
    let last_of_epoch_0 = get_epoch(&mut fullnode, 0, &["last_checkpoint"])
        .await
        .last_checkpoint
        .unwrap();

    let mut serve_kv_rpc_through = async |checkpoint| {
        release_checkpoints(
            fullnode_dir.path(),
            kv_rpc_dir.path(),
            &mut next_to_release,
            checkpoint,
        );
        offchain
            .wait_for_kv_rpc(checkpoint, Duration::from_secs(60))
            .await
            .unwrap_or_else(|_| panic!("kv-rpc serves checkpoint {checkpoint}"));
    };

    // kv-rpc at L - 1: epoch 0 is running, at its state there.
    serve_kv_rpc_through(last_of_epoch_0 - 1).await;
    let before_end = get_latest_epoch(&mut kv_rpc, LATEST_STATE_FIELDS).await;
    assert_eq!(before_end.epoch, Some(0));
    assert_eq!(before_end.last_checkpoint, None);
    let state_before_end = before_end.system_state.expect("system_state");
    assert_eq!(state_before_end.epoch, Some(0));
    let start_snapshot = get_epoch(&mut fullnode, 0, &["system_state"])
        .await
        .system_state
        .expect("system_state");
    assert_ne!(
        state_before_end, start_snapshot,
        "the stake changes epoch 0's state after its start",
    );
    let committee_0 = get_epoch(&mut fullnode, 0, &["committee"]).await.committee;
    assert!(committee_0.is_some());
    assert_eq!(before_end.committee, committee_0);

    // kv-rpc at L: epoch 0 has ended, and its state is still the one at L - 1.
    serve_kv_rpc_through(last_of_epoch_0).await;
    let at_end = get_latest_epoch(&mut kv_rpc, LATEST_STATE_FIELDS).await;
    assert_eq!(at_end.epoch, Some(0));
    assert_eq!(at_end.last_checkpoint, Some(last_of_epoch_0));
    assert_eq!(at_end.system_state.as_ref(), Some(&state_before_end));
    assert_eq!(at_end.committee, committee_0);

    // kv-rpc at epoch 1's first checkpoint: its latest epoch is epoch 1, at epoch 1's state, and
    // epoch 0 is past and serves its start snapshot, as on the fullnode.
    let first_of_epoch_1 = last_of_epoch_0 + 1;
    tokio::time::timeout(Duration::from_secs(60), async {
        while latest_checkpoint(&mut fullnode).await < first_of_epoch_1 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("epoch 1 has a checkpoint");
    serve_kv_rpc_through(first_of_epoch_1).await;
    let in_epoch_1 = get_latest_epoch(&mut kv_rpc, LATEST_STATE_FIELDS).await;
    assert_eq!(in_epoch_1.epoch, Some(1));
    assert_eq!(in_epoch_1.last_checkpoint, None);
    assert_eq!(in_epoch_1.system_state.unwrap().epoch, Some(1));
    assert_eq!(in_epoch_1.committee.unwrap().epoch, Some(1));

    let served = get_epoch(&mut fullnode, 0, STORED_FIELDS).await;
    // The comparison only means something if the ended epoch's end fields are set.
    assert!(served.transaction_end_exclusive.is_some());
    assert!(served.system_epoch_info.is_some());
    assert_eq!(get_epoch(&mut kv_rpc, 0, STORED_FIELDS).await, served);
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
