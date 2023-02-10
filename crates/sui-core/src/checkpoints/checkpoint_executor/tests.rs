// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use fastcrypto::traits::KeyPair;
use tempfile::tempdir;

use std::{sync::Arc, time::Duration};

use broadcast::{Receiver, Sender};
use sui_types::committee::ProtocolVersion;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use tokio::{sync::broadcast, time::timeout};

use crate::{authority::AuthorityState, checkpoints::CheckpointStore};

use sui_network::state_sync::test_utils::{empty_contents, CommitteeFixture};

/// Test checkpoint executor happy path, test that checkpoint executor correctly
/// picks up where it left off in the event of a mid-epoch node crash.
#[tokio::test]
pub async fn test_checkpoint_executor_crash_recovery() {
    let buffer_size = num_cpus::get() * 2;
    let tempdir = tempdir().unwrap();
    let checkpoint_store = CheckpointStore::new(tempdir.path());

    let (state, mut executor, checkpoint_sender, committee): (
        Arc<AuthorityState>,
        CheckpointExecutor,
        Sender<VerifiedCheckpoint>,
        CommitteeFixture,
    ) = init_executor_test(buffer_size, checkpoint_store.clone()).await;

    assert!(matches!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap(),
        None,
    ));
    let checkpoints = sync_new_checkpoints(
        &checkpoint_store,
        &checkpoint_sender,
        2 * buffer_size,
        None,
        &committee,
    );

    let epoch_store = state.epoch_store().clone();
    let executor_handle =
        spawn_monitored_task!(async move { executor.run_epoch(epoch_store).await });
    tokio::time::sleep(Duration::from_secs(5)).await;

    // ensure we executed all synced checkpoints
    let highest_executed = checkpoint_store
        .get_highest_executed_checkpoint_seq_number()
        .unwrap()
        .expect("Expected highest executed to not be None");
    assert_eq!(highest_executed, 2 * (buffer_size as u64) - 1,);

    // Simulate node restart
    executor_handle.abort();

    // sync more checkpoints in the meantime
    let _ = sync_new_checkpoints(
        &checkpoint_store,
        &checkpoint_sender,
        2 * buffer_size,
        Some(checkpoints.last().cloned().unwrap()),
        &committee,
    );

    // restart checkpoint executor and ensure that it picks
    // up where it left off
    let mut executor = CheckpointExecutor::new_for_tests(
        checkpoint_sender.subscribe(),
        checkpoint_store.clone(),
        state.database.clone(),
        state.transaction_manager().clone(),
    );

    let epoch_store = state.epoch_store().clone();
    let executor_handle =
        spawn_monitored_task!(async move { executor.run_epoch(epoch_store).await });
    tokio::time::sleep(Duration::from_secs(5)).await;

    let highest_executed = checkpoint_store
        .get_highest_executed_checkpoint_seq_number()
        .unwrap()
        .expect("Expected highest executed to not be None");
    assert_eq!(highest_executed, 4 * (buffer_size as u64) - 1);

    executor_handle.abort();
}

/// Test that checkpoint execution correctly signals end of epoch after
/// receiving last checkpoint of epoch, then resumes executing cehckpoints
/// from the next epoch if called after reconfig
#[tokio::test]
pub async fn test_checkpoint_executor_cross_epoch() {
    let buffer_size = 10;
    let num_to_sync_per_epoch = (buffer_size * 2) as usize;
    let tempdir = tempdir().unwrap();
    let checkpoint_store = CheckpointStore::new(tempdir.path());

    let (authority_state, mut executor, checkpoint_sender, first_committee): (
        Arc<AuthorityState>,
        CheckpointExecutor,
        Sender<VerifiedCheckpoint>,
        CommitteeFixture,
    ) = init_executor_test(buffer_size, checkpoint_store.clone()).await;

    assert!(matches!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap(),
        None,
    ));

    // sync 20 checkpoints
    let cold_start_checkpoints = sync_new_checkpoints(
        &checkpoint_store,
        &checkpoint_sender,
        num_to_sync_per_epoch,
        None,
        &first_committee,
    );

    // sync end of epoch checkpoint
    let last_executed_checkpoint = cold_start_checkpoints.last().cloned().unwrap();
    let (end_of_epoch_checkpoint, second_committee) = sync_end_of_epoch_checkpoint(
        &checkpoint_store,
        &checkpoint_sender,
        last_executed_checkpoint,
        &first_committee,
    );

    // sync 20 more checkpoints
    let next_epoch_checkpoints = sync_new_checkpoints(
        &checkpoint_store,
        &checkpoint_sender,
        num_to_sync_per_epoch,
        Some(end_of_epoch_checkpoint),
        &second_committee,
    );

    // sync end of epoch checkpoint
    let last_executed_checkpoint = next_epoch_checkpoints.last().cloned().unwrap();
    let (_end_of_epoch_checkpoint, _third_committee) = sync_end_of_epoch_checkpoint(
        &checkpoint_store,
        &checkpoint_sender,
        last_executed_checkpoint,
        &second_committee,
    );

    // Ensure executor reaches end of epoch in a timely manner
    timeout(Duration::from_secs(5), async {
        executor
            .run_epoch(authority_state.epoch_store().clone())
            .await;
    })
    .await
    .unwrap();

    // We should have synced up to epoch boundary
    assert_eq!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap(),
        num_to_sync_per_epoch as u64,
    );

    authority_state
        .reconfigure(second_committee.committee().clone(), 0)
        .await
        .unwrap();

    // checkpoint execution should resume starting at checkpoints
    // of next epoch
    timeout(Duration::from_secs(5), async {
        executor
            .run_epoch(authority_state.epoch_store().clone())
            .await;
    })
    .await
    .unwrap();

    assert_eq!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap(),
        2 * num_to_sync_per_epoch as u64 + 1,
    );
}

/// Test that if we crash at end of epoch / during reconfig, we recover on startup
/// by starting at the old epoch and immediately retrying reconfig
#[tokio::test]
pub async fn test_reconfig_crash_recovery() {
    let tempdir = tempdir().unwrap();
    let checkpoint_store = CheckpointStore::new(tempdir.path());

    // new Node (syncing from checkpoint 0)
    let (authority_state, mut executor, checkpoint_sender, first_committee): (
        Arc<AuthorityState>,
        CheckpointExecutor,
        Sender<VerifiedCheckpoint>,
        CommitteeFixture,
    ) = init_executor_test(
        10, /* StateSync -> Executor channel buffer size */
        checkpoint_store.clone(),
    )
    .await;

    assert!(matches!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap(),
        None,
    ));

    // sync 1 checkpoint
    let checkpoint = sync_new_checkpoints(
        &checkpoint_store,
        &checkpoint_sender,
        1,
        None,
        &first_committee,
    )
    .pop()
    .unwrap();

    // sync end of epoch checkpoint
    let (end_of_epoch_checkpoint, second_committee) = sync_end_of_epoch_checkpoint(
        &checkpoint_store,
        &checkpoint_sender,
        checkpoint,
        &first_committee,
    );
    // sync 1 more checkpoint
    let _next_epoch_checkpoints = sync_new_checkpoints(
        &checkpoint_store,
        &checkpoint_sender,
        1,
        Some(end_of_epoch_checkpoint.clone()),
        &second_committee,
    );

    timeout(Duration::from_secs(1), async {
        executor
            .run_epoch(authority_state.epoch_store().clone())
            .await;
    })
    .await
    .unwrap();

    // Check that we stopped execution at epoch boundary
    assert_eq!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap(),
        end_of_epoch_checkpoint.sequence_number(),
    );

    // Drop and re-istantiate checkpoint executor without performing reconfig. This
    // is logically equivalent to reconfig crashing and the node restarting, in which
    // case executor should be able to infer that, rather than beginning execution of
    // the next epoch, we should immediately exit so that reconfig can be reattempted.
    drop(executor);
    let mut executor = CheckpointExecutor::new_for_tests(
        checkpoint_sender.subscribe(),
        checkpoint_store.clone(),
        authority_state.database.clone(),
        authority_state.transaction_manager().clone(),
    );

    timeout(Duration::from_millis(200), async {
        executor
            .run_epoch(authority_state.epoch_store().clone())
            .await;
    })
    .await
    .unwrap();

    // Check that we have still not gone beyond epoch boundary
    assert_eq!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap(),
        end_of_epoch_checkpoint.sequence_number(),
    );
}

async fn init_executor_test(
    buffer_size: usize,
    store: Arc<CheckpointStore>,
) -> (
    Arc<AuthorityState>,
    CheckpointExecutor,
    Sender<VerifiedCheckpoint>,
    CommitteeFixture,
) {
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir).build();
    let genesis = network_config.genesis;
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let keypair = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();
    let state =
        AuthorityState::new_for_testing(committee.committee().clone(), &keypair, None, &genesis)
            .await;

    let (checkpoint_sender, _): (Sender<VerifiedCheckpoint>, Receiver<VerifiedCheckpoint>) =
        broadcast::channel(buffer_size);
    let executor = CheckpointExecutor::new_for_tests(
        checkpoint_sender.subscribe(),
        store.clone(),
        state.database.clone(),
        state.transaction_manager().clone(),
    );
    (state, executor, checkpoint_sender, committee)
}

/// Creates and simulates syncing of a new checkpoint by StateSync, i.e. new
/// checkpoint is persisted, along with its contents, highest synced checkpoint
/// watermark is updated, and message is broadcasted notifying of the newly synced
/// checkpoint. Returns created checkpoints
fn sync_new_checkpoints(
    checkpoint_store: &CheckpointStore,
    sender: &Sender<VerifiedCheckpoint>,
    number_of_checkpoints: usize,
    previous_checkpoint: Option<VerifiedCheckpoint>,
    committee: &CommitteeFixture,
) -> Vec<VerifiedCheckpoint> {
    let (ordered_checkpoints, _sequence_number_to_digest, _checkpoints) =
        committee.make_checkpoints(number_of_checkpoints, previous_checkpoint);

    for checkpoint in ordered_checkpoints.iter() {
        sync_checkpoint(checkpoint, checkpoint_store, sender);
    }

    ordered_checkpoints
}

fn sync_end_of_epoch_checkpoint(
    checkpoint_store: &CheckpointStore,
    sender: &Sender<VerifiedCheckpoint>,
    previous_checkpoint: VerifiedCheckpoint,
    committee: &CommitteeFixture,
) -> (VerifiedCheckpoint, CommitteeFixture) {
    let new_committee =
        CommitteeFixture::generate(rand::rngs::OsRng, committee.committee().epoch + 1, 4);
    let (_sequence_number, _digest, checkpoint) = committee.make_end_of_epoch_checkpoint(
        previous_checkpoint,
        Some(EndOfEpochData {
            next_epoch_committee: new_committee.committee().voting_rights.clone(),
            next_epoch_protocol_version: ProtocolVersion::MIN,
        }),
    );
    sync_checkpoint(&checkpoint, checkpoint_store, sender);

    (checkpoint, new_committee)
}

fn sync_checkpoint(
    checkpoint: &VerifiedCheckpoint,
    checkpoint_store: &CheckpointStore,
    sender: &Sender<VerifiedCheckpoint>,
) {
    checkpoint_store
        .insert_verified_checkpoint(checkpoint.clone())
        .unwrap();
    checkpoint_store
        .insert_checkpoint_contents(empty_contents())
        .unwrap();
    checkpoint_store
        .update_highest_synced_checkpoint(checkpoint)
        .unwrap();
    sender.send(checkpoint.clone()).unwrap();
}
