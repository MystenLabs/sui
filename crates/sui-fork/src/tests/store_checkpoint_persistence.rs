// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoint persistence tests for [`crate::store::DataStore`]. Lives under
//! `src/tests/` but is a child of the `store` module via a `#[path]` wiring
//! in `store.rs`, so it retains `super::*` access to `pub(crate)` items.

use std::num::NonZeroUsize;

use rand::rngs::OsRng;
use simulacrum::Simulacrum;
use simulacrum::store::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::ObjectID;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

use super::*;

fn build_checkpoint(sequence: u64) -> (VerifiedCheckpoint, CheckpointContents) {
    let data = TestCheckpointBuilder::new(sequence).build_checkpoint();
    (
        VerifiedCheckpoint::new_unchecked(data.summary),
        data.contents,
    )
}

#[test]
fn insert_checkpoint_pair_persists_both_to_disk() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut store = DataStore::new_for_testing(temp.path().to_path_buf());
    let (checkpoint, contents) = build_checkpoint(42);
    let sequence = checkpoint.data().sequence_number;

    store.insert_checkpoint(checkpoint.clone());
    store.insert_checkpoint_contents(contents.clone());

    let loaded = SimulatorStore::get_checkpoint_by_sequence_number(&store, sequence)
        .expect("checkpoint should be persisted");
    assert_eq!(loaded.data(), checkpoint.data());

    let loaded_contents = SimulatorStore::get_checkpoint_contents(&store, contents.digest())
        .expect("contents should be persisted");
    assert_eq!(loaded_contents.digest(), contents.digest());
}

#[test]
fn post_fork_sequence_miss_returns_none_without_remote() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = DataStore::new_for_testing(temp.path().to_path_buf());
    // `new_for_testing` pins `forked_at_checkpoint = 0`, so any positive
    // sequence is "post-fork". The dummy GraphQL endpoint is unreachable,
    // so reaching the network would surface as an error here.
    let result = DataStore::get_checkpoint_by_sequence_number(&store, 42)
        .expect("post-fork miss should short-circuit before the remote");
    assert!(result.is_none());
}

#[test]
fn insert_checkpoint_and_contents_are_independent() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut store = DataStore::new_for_testing(temp.path().to_path_buf());
    let (checkpoint, contents) = build_checkpoint(5);

    // Insert contents first, then the summary: contents are content-addressed
    // so there is no ordering dependency between the two halves.
    store.insert_checkpoint_contents(contents.clone());
    assert_eq!(
        SimulatorStore::get_checkpoint_contents(&store, contents.digest())
            .expect("contents should be retrievable by digest")
            .digest(),
        contents.digest(),
    );
    assert!(
        SimulatorStore::get_highest_checkpint(&store).is_none(),
        "latest marker should not advance until a summary is inserted",
    );

    store.insert_checkpoint(checkpoint.clone());
    let highest = SimulatorStore::get_highest_checkpint(&store)
        .expect("latest marker should advance after summary insert");
    assert_eq!(
        highest.data().sequence_number,
        checkpoint.data().sequence_number,
    );
}

#[tokio::test]
async fn resumed_simulacrum_builds_next_checkpoint_after_highest_local_checkpoint() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut rng = OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .build();
    let mut store = DataStore::new_for_testing(temp.path().to_path_buf());
    let written: BTreeMap<ObjectID, Object> = config
        .genesis
        .objects()
        .iter()
        .map(|object| (object.id(), object.clone()))
        .collect();
    store.update_objects(written, vec![]);

    let (checkpoint, contents) = build_checkpoint(5);
    store.insert_checkpoint(checkpoint.clone());
    store.insert_checkpoint_contents(contents);

    let keystore = KeyStore::from_network_config(&config);
    let mut sim = Simulacrum::new_from_custom_state(
        keystore,
        store
            .get_highest_verified_checkpoint()
            .expect("highest checkpoint lookup should not fail")
            .expect("highest checkpoint should exist"),
        config.genesis.sui_system_object(),
        &config,
        store,
        rng,
    );

    let next = sim.create_checkpoint();
    assert_eq!(
        next.data().sequence_number,
        checkpoint.data().sequence_number + 1,
    );
}
