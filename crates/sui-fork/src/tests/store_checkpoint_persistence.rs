// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoint persistence tests for [`crate::store::DataStore`]. Lives under
//! `src/tests/` but is a child of the `store` module via a `#[path]` wiring
//! in `store.rs`, so it retains `super::*` access to `pub(crate)` items.

use std::num::NonZeroUsize;
use std::path::Path;

use fastcrypto::encoding::Base64 as FastCryptoBase64;
use fastcrypto::encoding::Encoding;
use rand::rngs::OsRng;
use serde_json::json;
use simulacrum::Simulacrum;
use simulacrum::store::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::ObjectID;
use sui_types::digests::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::storage::ReadStore;
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_partial_json;
use wiremock::matchers::method;
use wiremock::matchers::path;

use crate::runtime::ForkRuntime;

use super::*;

fn build_checkpoint(sequence: u64) -> (VerifiedCheckpoint, CheckpointContents) {
    let data = TestCheckpointBuilder::new(sequence).build_checkpoint();
    (
        VerifiedCheckpoint::new_unchecked(data.summary),
        data.contents,
    )
}

fn open_test_runtime(root: &Path, forked_at_checkpoint: CheckpointSequenceNumber) -> ForkRuntime {
    ForkRuntime::open(
        root,
        "custom".to_owned(),
        forked_at_checkpoint,
        CheckpointDigest::new([9; 32]).into(),
    )
    .expect("fork runtime should open")
}

fn test_data_store(root: &Path) -> (DataStore, ForkRuntime) {
    let runtime = open_test_runtime(root, 0);
    let store = DataStore::new_for_testing(root.to_path_buf(), runtime.fork_rpc_store());
    (store, runtime)
}

fn test_data_store_with_remote(
    root: &Path,
    gql_url: String,
    forked_at_checkpoint: CheckpointSequenceNumber,
) -> (DataStore, ForkRuntime) {
    let runtime = open_test_runtime(root, forked_at_checkpoint);
    let store = DataStore::new_for_testing_with_remote(
        root.to_path_buf(),
        gql_url,
        forked_at_checkpoint,
        runtime.fork_rpc_store(),
    );
    (store, runtime)
}

fn checkpoint_response_body(
    checkpoint: &VerifiedCheckpoint,
    contents: &CheckpointContents,
) -> serde_json::Value {
    json!({
        "data": {
            "checkpoint": {
                "summaryBcs": FastCryptoBase64::from_bytes(
                    &bcs::to_bytes(checkpoint.data()).expect("summary should serialize"),
                )
                .encoded(),
                "contentBcs": FastCryptoBase64::from_bytes(
                    &bcs::to_bytes(contents).expect("contents should serialize"),
                )
                .encoded(),
                "validatorSignatures": {
                    "signature": FastCryptoBase64::from_bytes(
                        checkpoint.auth_sig().signature.as_ref(),
                    )
                    .encoded(),
                    "signersMap": checkpoint
                        .auth_sig()
                        .signers_map
                        .iter()
                        .map(|index| i32::try_from(index).expect("signer index fits in i32"))
                        .collect::<Vec<_>>(),
                },
            }
        }
    })
}

async fn mount_checkpoint_mock(
    server: &MockServer,
    checkpoint: &VerifiedCheckpoint,
    contents: &CheckpointContents,
) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(json!({
            "variables": {
                "sequenceNumber": checkpoint.data().sequence_number,
            }
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(checkpoint_response_body(checkpoint, contents)),
        )
        .mount(server)
        .await;
}

#[test]
fn insert_checkpoint_pair_saves_both_to_rpc_store() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (mut store, _runtime) = test_data_store(temp.path());
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
    let (store, _runtime) = test_data_store(temp.path());
    // `new_for_testing` pins `forked_at_checkpoint = 0`, so any positive
    // sequence is "post-fork". The dummy GraphQL endpoint is unreachable,
    // so reaching the network would surface as an error here.
    let result = DataStore::get_checkpoint_by_sequence_number(&store, 42)
        .expect("post-fork miss should short-circuit before the remote");
    assert!(result.is_none());
}

#[tokio::test]
async fn remote_checkpoint_fetch_saves_into_rpc_store() {
    let server = MockServer::start().await;
    let (checkpoint, contents) = build_checkpoint(11);
    mount_checkpoint_mock(&server, &checkpoint, &contents).await;

    let temp = tempfile::tempdir().expect("tempdir");
    let (store, runtime) = test_data_store_with_remote(temp.path(), server.uri(), 11);

    let loaded = DataStore::get_checkpoint_by_sequence_number(&store, 11)
        .expect("remote checkpoint fetch should succeed")
        .expect("checkpoint should exist");
    assert_eq!(loaded.data(), checkpoint.data());

    let reader = runtime.reader();
    let rpc_checkpoint = ReadStore::get_checkpoint_by_sequence_number(&reader, 11)
        .expect("checkpoint should be saved in rpc store");
    assert_eq!(rpc_checkpoint.data(), checkpoint.data());
    assert!(
        ReadStore::get_checkpoint_by_digest(&reader, checkpoint.digest()).is_some(),
        "checkpoint digest index should be saved",
    );
    let rpc_contents = ReadStore::get_checkpoint_contents_by_sequence_number(&reader, 11)
        .expect("checkpoint contents should be saved in rpc store");
    assert_eq!(rpc_contents.digest(), contents.digest());
    assert_eq!(
        rpc_contents.clone().into_inner(),
        contents.clone().into_inner()
    );

    let fallback_temp = tempfile::tempdir().expect("tempdir");
    let fallback_store = DataStore::new_for_testing_with_remote(
        fallback_temp.path().to_path_buf(),
        "http://localhost:1".to_owned(),
        checkpoint.data().sequence_number,
        runtime.fork_rpc_store(),
    );

    let fallback_checkpoint = DataStore::get_checkpoint_by_sequence_number(&fallback_store, 11)
        .expect("rpc-store checkpoint lookup should succeed")
        .expect("checkpoint should be read from rpc store");
    assert_eq!(fallback_checkpoint.data(), checkpoint.data());
    assert!(
        DataStore::get_checkpoint_by_digest(&fallback_store, checkpoint.digest())
            .expect("rpc-store digest lookup should succeed")
            .is_some(),
        "checkpoint digest should be read from rpc store",
    );
    let fallback_contents =
        DataStore::get_checkpoint_contents_by_sequence_number(&fallback_store, 11)
            .expect("rpc-store contents lookup should succeed")
            .expect("contents should be read from rpc store");
    assert_eq!(fallback_contents.digest(), contents.digest());
    assert_eq!(fallback_contents.into_inner(), contents.into_inner());
}

#[test]
fn insert_checkpoint_summary_is_pending_until_contents_arrive() {
    let temp = tempfile::tempdir().expect("tempdir");
    let (mut store, _runtime) = test_data_store(temp.path());
    let (checkpoint, contents) = build_checkpoint(5);

    store.insert_checkpoint(checkpoint.clone());
    assert!(
        SimulatorStore::get_highest_checkpint(&store).is_none(),
        "summary alone should remain pending until contents arrive",
    );

    store.insert_checkpoint_contents(contents.clone());
    let highest = SimulatorStore::get_highest_checkpint(&store)
        .expect("latest marker should advance after matching contents insert");
    assert_eq!(
        highest.data().sequence_number,
        checkpoint.data().sequence_number,
    );
    assert_eq!(
        SimulatorStore::get_checkpoint_contents(&store, contents.digest())
            .expect("contents should be retrievable by digest")
            .digest(),
        contents.digest(),
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

    // Session 1: persist genesis objects and an advanced local tip, then drop
    // every handle so the data dir can be reopened like a real restart.
    let tip = {
        let (mut store, _runtime) = test_data_store(temp.path());
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
        checkpoint
    };

    // Session 2: resume from the reopened store. The production base-checkpoint
    // selection must pick the local tip, not the fork point — re-seeding from
    // the fork point would rebuild an already-persisted sequence number and
    // panic the seal.
    let (store, _runtime) = test_data_store(temp.path());
    let base = crate::startup::resume_base_checkpoint(&store)
        .expect("resume base checkpoint should resolve");
    assert_eq!(base.data().sequence_number, tip.data().sequence_number);

    let keystore = KeyStore::from_network_config(&config);
    let mut sim = Simulacrum::new_from_custom_state(
        keystore,
        base.clone(),
        config.genesis.sui_system_object(),
        &config,
        store,
        rng,
    );

    let next = sim.create_checkpoint();
    assert_eq!(next.data().sequence_number, tip.data().sequence_number + 1,);
    assert_eq!(next.data().previous_digest, Some(*base.digest()));
    assert!(
        ReadStore::get_checkpoint_by_sequence_number(
            sim.store().rpc_store().reader(),
            next.data().sequence_number,
        )
        .is_some(),
        "resumed checkpoint should seal into the rpc-store",
    );
}
