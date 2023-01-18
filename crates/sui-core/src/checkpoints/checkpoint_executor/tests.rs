// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use fastcrypto::traits::KeyPair;
use tempfile::tempdir;

use std::{sync::Arc, time::Duration};

use broadcast::{Receiver, Sender};
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

    // new Node (syncing from checkpoint 0)
    let cold_start_checkpoints = {
        let (_state, executor, checkpoint_sender, committee): (
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
        let (executor_handle, _reconfig_channel) = executor.start().unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;

        // dropping the channel will cause the checkpoint executor process to exit (gracefully)
        drop(checkpoint_sender);
        timeout(Duration::from_secs(1), async {
            executor_handle
                .join()
                .await
                .expect("Should exit gracefully");
        })
        .await
        .unwrap();

        assert!(matches!(
            checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap(),
            Some(highest) if highest == 2 * (buffer_size as u64) - 1,
        ));

        checkpoints
    };

    // Node shutdown, syncing from checkpoint > 0
    {
        let last_executed_checkpoint = cold_start_checkpoints.last().cloned().unwrap();

        let (_state, executor, checkpoint_sender, committee): (
            Arc<AuthorityState>,
            CheckpointExecutor,
            Sender<VerifiedCheckpoint>,
            CommitteeFixture,
        ) = init_executor_test(buffer_size, checkpoint_store.clone()).await;

        assert!(matches!(
            checkpoint_store
                .get_highest_executed_checkpoint_seq_number()
                .unwrap(),
            Some(seq_num) if seq_num == last_executed_checkpoint.sequence_number(),
        ));
        // Start syncing new checkpoints from the last checkpoint before
        // previous shutdown
        let _ = sync_new_checkpoints(
            &checkpoint_store,
            &checkpoint_sender,
            2 * buffer_size,
            Some(last_executed_checkpoint),
            &committee,
        );
        let (executor_handle, _reconfig_channel) = executor.start().unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;

        // dropping the channel will cause the checkpoint executor process to exit (gracefully)
        drop(checkpoint_sender);
        timeout(Duration::from_secs(1), async {
            executor_handle
                .join()
                .await
                .expect("Should exit gracefully");
        })
        .await
        .unwrap();

        assert!(matches!(
            checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap(),
            Some(highest) if highest == 4 * (buffer_size as u64) - 1,
        ));
    }
}

/// Test that checkpoint execution correctly signals end of epoch after
/// receiving last checkpoint of epoch, pauses execution until reconfig,
/// then resumes checkpoint execution after reconfig.
#[tokio::test]
pub async fn test_checkpoint_executor_cross_epoch() {
    let buffer_size = 10;
    let num_to_sync_per_epoch = (buffer_size * 2) as usize;
    let tempdir = tempdir().unwrap();
    let checkpoint_store = CheckpointStore::new(tempdir.path());

    let (authority_state, executor, checkpoint_sender, first_committee): (
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
    let _next_epoch_checkpoints = sync_new_checkpoints(
        &checkpoint_store,
        &checkpoint_sender,
        num_to_sync_per_epoch,
        Some(end_of_epoch_checkpoint),
        &second_committee,
    );

    let (_handle, mut reconfig_channel) = executor.start().unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;

    // We should have synced up to epoch boundary
    assert!(matches!(
        checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap(),
        Some(highest) if highest == (num_to_sync_per_epoch as u64),
    ));

    // Ensure we have end of epoch notification
    let next_committee = reconfig_channel.recv().await.unwrap();
    assert_eq!(second_committee.committee(), &next_committee);

    authority_state
        .reconfigure(second_committee.committee().clone())
        .await
        .unwrap();

    // checkpoint execution should resume
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(matches!(
        checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap(),
        Some(highest) if highest == (2 * num_to_sync_per_epoch as u64),
    ));
}

/// Test that if we crash at end of epoch / during reconfig, we recover on startup
/// by starting at the old epoch and immediately retrying reconfig
#[tokio::test]
pub async fn test_reconfig_crash_recovery() {
    let tempdir = tempdir().unwrap();
    let checkpoint_store = CheckpointStore::new(tempdir.path());

    // new Node (syncing from checkpoint 0)
    let (authority_state, executor, checkpoint_sender, first_committee): (
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

    let (executor_handle, mut reconfig_channel) = executor.start().unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Check that we paused execution at epoch boundary
    assert_eq!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap(),
        end_of_epoch_checkpoint.sequence_number(),
    );

    // Check that we have end of epoch notification - this will trigger reconfig
    let next_committee = reconfig_channel.recv().await.unwrap();
    assert_eq!(second_committee.committee(), &next_committee);

    // Simulate reconfig crash by dropping the channel and aborting the executor task,
    // causing checkpoint executor to exit ungracefully.
    // TODO drop won't work here because we are paused for reconfig and thus are no longer
    // polling the recv channel. Instead abort() the task
    drop(checkpoint_sender);
    executor_handle.event_loop_handle().abort();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Node restart
    let (checkpoint_sender, _): (Sender<VerifiedCheckpoint>, Receiver<VerifiedCheckpoint>) =
        broadcast::channel(10);
    let executor = CheckpointExecutor::new_for_tests(
        checkpoint_sender.subscribe(),
        checkpoint_store.clone(),
        authority_state.clone(),
    );
    let (_handle, mut reconfig_channel) = executor.start().unwrap();

    // Check that post-startup, we remain paused for reconfig
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap(),
        end_of_epoch_checkpoint.sequence_number(),
    );

    // Check that end of epoch is re-signaled so that we re-attempt reconfig
    let next_committee = reconfig_channel.recv().await.unwrap();
    assert_eq!(second_committee.committee(), &next_committee);

    authority_state
        .reconfigure(second_committee.committee().clone())
        .await
        .unwrap();

    // checkpoint execution should resume
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap(),
        end_of_epoch_checkpoint.sequence_number() + 1,
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
        state.clone(),
    );
    (state, executor, checkpoint_sender, committee)
}

/// Creates and simulates syncing of a new checkpoint by StateSync, i.e. new
/// checkpoint is persisted, along with its contents, highest synced checkpoint
/// watermark is udpated, and message is broadcasted notifying of the newly synced
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
        new_committee.committee().voting_rights.clone(),
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
