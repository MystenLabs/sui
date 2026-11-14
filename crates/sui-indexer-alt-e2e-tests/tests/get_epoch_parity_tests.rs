// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `LedgerService/GetEpoch` on a fullnode and on kv-rpc indexing the same chain.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
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

/// A fullnode and kv-rpc serve the same stored and end-of-epoch fields for an ended and a running
/// epoch, and the same system state for both.
///
/// Around the end of epoch 0 at its last checkpoint L, the latest epoch's `system_state` is
/// epoch 0's state until a server serves a checkpoint of epoch 1: at L it is the state at L - 1,
/// since the change-epoch transaction in L has already written epoch 1's state. kv-rpc is held at
/// L - 1, L and then epoch 1 by releasing checkpoints to it one step at a time. The fullnode
/// cannot be held, so it is polled while the epoch changes, and every answer it gives in that
/// window is checked.
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
    onchain.wait_for_rpc_index_ready().await;

    let polling = Arc::new(AtomicBool::new(true));
    let poller = tokio::spawn({
        let polling = polling.clone();
        let mut fullnode = fullnode.clone();
        async move {
            let mut samples = vec![];
            while polling.load(Ordering::Relaxed) {
                samples.push(get_latest_epoch(&mut fullnode, LATEST_STATE_FIELDS).await);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            samples
        }
    });

    onchain.trigger_reconfiguration().await;
    onchain.wait_for_rpc_index_ready().await;
    let last_of_epoch_0 = get_epoch(&mut fullnode, 0, &["last_checkpoint"])
        .await
        .last_checkpoint
        .unwrap();
    // Wait until the fullnode has executed and indexed a checkpoint of epoch 1.
    let first_of_epoch_1 = last_of_epoch_0 + 1;
    tokio::time::timeout(Duration::from_secs(60), async {
        while latest_checkpoint(&mut fullnode).await < first_of_epoch_1 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("epoch 1 has a checkpoint");
    onchain.wait_for_rpc_index_ready().await;
    polling.store(false, Ordering::Relaxed);
    let fullnode_samples = poller.await.expect("the fullnode answers every poll");

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

    // Every fullnode answer while the epoch changed is the state of the epoch it names, and the
    // epoch never goes back. At least one is given at L, each the same state kv-rpc gives at L.
    let mut sampled_at_end = 0;
    for pair in fullnode_samples.windows(2) {
        assert!(pair[1].epoch >= pair[0].epoch, "{pair:?}");
    }
    for sample in &fullnode_samples {
        let state = sample.system_state.as_ref().expect("system_state");
        assert_eq!(state.epoch, sample.epoch, "{sample:?}");
        assert_eq!(
            sample
                .committee
                .as_ref()
                .and_then(|committee| committee.epoch),
            sample.epoch,
            "{sample:?}",
        );
        if sample.last_checkpoint.is_some() {
            assert_eq!(sample.last_checkpoint, Some(last_of_epoch_0));
            assert_eq!(state, &state_before_end);
            sampled_at_end += 1;
        }
    }
    eprintln!(
        "{} fullnode samples across the epoch change, {sampled_at_end} at epoch 0's last \
         checkpoint",
        fullnode_samples.len(),
    );
    assert!(
        sampled_at_end > 0,
        "no fullnode answer at epoch 0's last checkpoint {last_of_epoch_0}"
    );

    // kv-rpc in epoch 1, up to the fullnode's tip.
    let fullnode_tip = latest_checkpoint(&mut fullnode).await;
    serve_kv_rpc_through(fullnode_tip).await;
    onchain.wait_for_rpc_index_ready().await;

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

    // Nothing writes the system state after the change-epoch transaction, so both servers give
    // epoch 1's state as of their own served checkpoints, which is the same state.
    let running = get_epoch(&mut fullnode, 1, &["committee", "system_state"]).await;
    assert_eq!(running.system_state.as_ref().unwrap().epoch, Some(1));
    assert_eq!(
        get_epoch(&mut kv_rpc, 1, &["committee", "system_state"]).await,
        running
    );
    let latest = get_latest_epoch(&mut kv_rpc, LATEST_STATE_FIELDS).await;
    assert_eq!(latest.epoch, Some(1));
    assert_eq!(latest.committee, running.committee);
    assert_eq!(latest.system_state, running.system_state);
}
