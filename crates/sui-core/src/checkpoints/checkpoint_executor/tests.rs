// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use fastcrypto::traits::KeyPair;
use sui_types::{committee::Committee, crypto::AuthorityKeyPair};
use tempfile::tempdir;

use std::{sync::Arc, time::Duration};

use broadcast::{Receiver, Sender};
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use tokio::sync::broadcast;

use crate::{authority::AuthorityState, checkpoints::CheckpointStore};

use sui_network::state_sync::test_utils::{empty_contents, CommitteeFixture};

#[tokio::test]
pub async fn checkpoint_executor_test() {
    let buffer_size = num_cpus::get() * TASKS_PER_CORE * 2;
    let tempdir = tempdir().unwrap();
    let checkpoint_store = CheckpointStore::new(tempdir.path());

    // new Node (syncing from checkpoint 0)
    let cold_start_checkpoints = {
        let (executor, checkpoint_sender, committee): (
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
        let _handle = executor.start().unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
        // dropping the channel will cause the checkpoint executor process to exit (gracefully)
        drop(checkpoint_sender);

        assert!(matches!(
            checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap(),
            Some(highest) if highest == 2 * (buffer_size as u64) - 1,
        ));

        checkpoints
    };

    // Node shutdown, syncing from checkpoint > 0
    {
        let last_executed_checkpoint = cold_start_checkpoints.last().cloned().unwrap();

        let (executor, checkpoint_sender, committee): (
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
        let _handle = executor.start().unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
        // dropping the channel will cause the checkpoint executor process to exit (gracefully)
        drop(checkpoint_sender);

        assert!(matches!(
            checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap(),
            Some(highest) if highest == 4 * (buffer_size as u64) - 1,
        ));
    }

    // TODO test crossing epoch boundary
}

async fn init_executor_test(
    buffer_size: usize,
    store: Arc<CheckpointStore>,
) -> (
    CheckpointExecutor,
    Sender<VerifiedCheckpoint>,
    CommitteeFixture,
) {
    let (keypair, committee) = committee();
    let state = AuthorityState::new_for_testing(committee.clone(), &keypair, None, None).await;

    let (checkpoint_sender, _): (Sender<VerifiedCheckpoint>, Receiver<VerifiedCheckpoint>) =
        broadcast::channel(buffer_size);
    let executor =
        CheckpointExecutor::new_for_tests(checkpoint_sender.subscribe(), store.clone(), state);
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    (executor, checkpoint_sender, committee)
}

// Creates and simulates syncing of a new checkpoint by StateSync, i.e. new
// checkpoint is persisted, along with its contents, highest synced checkpoint
// watermark is udpated, and message is broadcasted notifying of the newly synced
// checkpoint. Returns created checkpoints
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

    ordered_checkpoints
}

fn committee() -> (AuthorityKeyPair, Committee) {
    use std::collections::BTreeMap;
    use sui_types::crypto::get_key_pair;

    let (_authority_address, authority_key): (_, AuthorityKeyPair) = get_key_pair();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    authorities.insert(
        /* address */ authority_key.public().into(),
        /* voting right */ 1,
    );
    (authority_key, Committee::new(0, authorities).unwrap())
}
