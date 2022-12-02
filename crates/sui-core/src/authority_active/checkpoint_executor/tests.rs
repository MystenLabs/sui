use super::*;
use fastcrypto::traits::KeyPair;
use sui_types::{committee::Committee, crypto::AuthorityKeyPair};
use tempfile::tempdir;
use tokio::sync::mpsc;

use std::{sync::Arc, time::Duration};

use broadcast::{Receiver, Sender};
use sui_metrics::spawn_monitored_task;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use tokio::sync::broadcast;

use crate::{
    authority::AuthorityState,
    checkpoints::{CheckpointMetrics, CheckpointStore},
};

use sui_network::state_sync::test_utils::{empty_contents, CommitteeFixture};

#[tokio::test]
pub async fn checkpoint_executor_test() {
    let buffer_size = num_cpus::get() * TASKS_PER_CORE * 2;
    let (executor, checkpoint_sender, committee, checkpoint_store): (
        CheckpointExecutor,
        Sender<VerifiedCheckpoint>,
        CommitteeFixture,
        Arc<CheckpointStore>,
    ) = init_executor_test(buffer_size).await;

    assert!(matches!(
        checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap(),
        None,
    ));
    sync_new_checkpoints(
        &checkpoint_store,
        &checkpoint_sender,
        2 * buffer_size,
        None,
        &committee,
    );
    let _handle = spawn_monitored_task!(async move { executor.run().await });
    tokio::time::sleep(Duration::from_secs(5)).await;
    // dropping the channel will cause the checkpoint executor process to exit (gracefully)
    drop(checkpoint_sender);

    assert!(matches!(
        checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap(),
        Some(highest) if highest == 2 * (buffer_size as u64) - 1,
    ));
}

async fn init_executor_test(
    buffer_size: usize,
) -> (
    CheckpointExecutor,
    Sender<VerifiedCheckpoint>,
    CommitteeFixture,
    Arc<CheckpointStore>,
) {
    let tempdir = tempdir().unwrap();
    let (keypair, committee) = committee();
    let (tx_reconfigure_consensus, _rx_reconfigure_consensus) = mpsc::channel(10);
    let state = Arc::new(
        AuthorityState::new_for_testing(
            committee.clone(),
            &keypair,
            None,
            None,
            tx_reconfigure_consensus,
        )
        .await,
    );

    let store = CheckpointStore::new(tempdir.path());
    let metrics = CheckpointMetrics::new_for_tests();
    let (checkpoint_sender, _): (Sender<VerifiedCheckpoint>, Receiver<VerifiedCheckpoint>) =
        broadcast::channel(buffer_size);
    let executor =
        CheckpointExecutor::new(checkpoint_sender.subscribe(), store.clone(), state, metrics)
            .unwrap();
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    (executor, checkpoint_sender, committee, store)
}

// Creates and simulates syncing of a new checkpoint by StateSync, i.e. new
// checkpoint is persisted, along with its contents, highest synced checkpoint
// watermark is udpated, and message is broadcasted notifying of the newly synced
// checkpoint
fn sync_new_checkpoints(
    checkpoint_store: &CheckpointStore,
    sender: &Sender<VerifiedCheckpoint>,
    number_of_checkpoints: usize,
    previous_checkpoint: Option<VerifiedCheckpoint>,
    committee: &CommitteeFixture,
) {
    let (ordered_checkpoints, _sequence_number_to_digest, _checkpoints) =
        committee.make_checkpoints(number_of_checkpoints, previous_checkpoint);

    for checkpoint in ordered_checkpoints {
        checkpoint_store
            .insert_verified_checkpoint(checkpoint.clone())
            .unwrap();
        checkpoint_store
            .insert_checkpoint_contents(empty_contents())
            .unwrap();
        checkpoint_store
            .update_highest_synced_checkpoint(&checkpoint)
            .unwrap();
        sender.send(checkpoint).unwrap();
    }
}

fn committee() -> (AuthorityKeyPair, Committee) {
    use std::collections::BTreeMap;
    use sui_types::crypto::get_key_pair;
    use sui_types::crypto::AuthorityPublicKeyBytes;

    let (_authority_address, authority_key): (_, AuthorityKeyPair) = get_key_pair();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    authorities.insert(
        /* address */ authority_key.public().into(),
        /* voting right */ 1,
    );
    (authority_key, Committee::new(0, authorities).unwrap())
}
