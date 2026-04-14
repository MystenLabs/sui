// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for [`crate::filesystem::FilesystemStore`]. Wired via
//! `#[cfg(test)] #[path = "tests/filesystem.rs"] mod tests;` so the file
//! lives under `src/tests/` but remains a child of the `filesystem` module
//! and has full `super::*` access to crate-private items.

use std::fs;

use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::{CheckpointContents, VerifiedCheckpoint};
use sui_types::object::MoveObject;
use sui_types::object::Object;
use sui_types::object::ObjectInner;
use sui_types::object::Owner;

use super::*;

fn test_store() -> (tempfile::TempDir, FilesystemStore) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = FilesystemStore::new_with_root(dir.path().to_path_buf());
    (dir, store)
}

fn make_object(id: ObjectID, version: u64) -> Object {
    let move_obj = MoveObject::new_gas_coin(SequenceNumber::from_u64(version), id, 1_000_000);
    ObjectInner {
        owner: Owner::Immutable,
        data: sui_types::object::Data::Move(move_obj),
        previous_transaction: TransactionDigest::genesis_marker(),
        storage_rebate: 0,
    }
    .into()
}

fn build_checkpoint(sequence: u64) -> (VerifiedCheckpoint, CheckpointContents) {
    let data = sui_types::test_checkpoint_data_builder::TestCheckpointBuilder::new(sequence)
        .build_checkpoint();
    let checkpoint = VerifiedCheckpoint::new_unchecked(data.summary);
    (checkpoint, data.contents)
}

#[test]
fn test_write_and_read_latest_object() {
    let (_dir, store) = test_store();
    let id = ObjectID::random();
    let obj = make_object(id, 5);

    store.write_object(&obj).unwrap();
    let loaded = store.get_latest_object(&id).unwrap();
    assert_eq!(loaded.clone().unwrap(), obj);
    assert_eq!(loaded.unwrap().version(), SequenceNumber::from_u64(5));
}

#[test]
fn test_write_and_read_object_at_version() {
    let (_dir, store) = test_store();
    let id = ObjectID::random();
    let obj = make_object(id, 5);

    store.write_object(&obj).unwrap();
    let loaded = store.get_object_at_version(&id, 5).unwrap();
    assert_eq!(loaded.unwrap(), obj);
}

#[test]
fn test_get_latest_object_returns_none_for_unknown_id() {
    let (_dir, store) = test_store();
    let result = store.get_latest_object(&ObjectID::random()).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_get_object_at_version_returns_none_for_unknown_version() {
    let (_dir, store) = test_store();
    let id = ObjectID::random();
    let obj = make_object(id, 5);
    store.write_object(&obj).unwrap();

    let result = store.get_object_at_version(&id, 99).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_latest_tracks_highest_written_version() {
    let (_dir, store) = test_store();
    let id = ObjectID::random();

    let v1 = make_object(id, 1);
    let v3 = make_object(id, 3);
    store.write_object(&v1).unwrap();
    store.write_object(&v3).unwrap();

    let latest = store.get_latest_object(&id).unwrap().unwrap();
    assert_eq!(latest, v3);

    // v1 is still accessible by version
    let old = store.get_object_at_version(&id, 1).unwrap().unwrap();
    assert_eq!(old, v1);
}

#[test]
fn test_get_highest_checkpoint_errors_when_dir_missing() {
    let (_dir, store) = test_store();
    let err = store.get_highest_checkpoint_sequence_number().unwrap_err();
    assert!(err.to_string().contains("does not exist"));
}

#[test]
fn test_get_highest_checkpoint_errors_when_latest_file_missing() {
    let (_dir, store) = test_store();
    fs::create_dir_all(store.checkpoints_dir()).unwrap();
    let err = store.get_highest_checkpoint_sequence_number().unwrap_err();
    assert!(err.to_string().contains("Latest file not found"));
}

#[test]
fn test_write_and_read_transaction_effects() {
    let (_dir, store) = test_store();
    let digest = TransactionDigest::random();
    let effects = TransactionEffects::default();

    store.write_transaction_effects(&digest, &effects).unwrap();
    let loaded = store.get_transaction_effects(&digest).unwrap();
    assert_eq!(loaded.unwrap(), effects);
}

#[test]
fn test_write_and_read_transaction_events() {
    let (_dir, store) = test_store();
    let digest = TransactionDigest::random();
    let events = TransactionEvents { data: vec![] };

    store.write_transaction_events(&digest, &events).unwrap();
    let loaded = store.get_transaction_events(&digest).unwrap();
    assert_eq!(loaded.unwrap(), events);
}

#[test]
fn test_get_transaction_returns_none_for_unknown_digest() {
    let (_dir, store) = test_store();
    let digest = TransactionDigest::random();

    assert!(store.get_transaction(&digest).unwrap().is_none());
    assert!(store.get_transaction_effects(&digest).unwrap().is_none());
    assert!(store.get_transaction_events(&digest).unwrap().is_none());
}

#[test]
fn test_write_and_read_checkpoint_by_sequence_and_digest() {
    let (_dir, store) = test_store();
    let (checkpoint, contents) = build_checkpoint(7);
    let sequence = checkpoint.data().sequence_number;

    store.write_checkpoint_summary(&checkpoint).unwrap();
    store.write_checkpoint_contents(&contents).unwrap();

    let by_seq = store
        .get_checkpoint_by_sequence_number(sequence)
        .unwrap()
        .unwrap();
    assert_eq!(by_seq.data(), checkpoint.data());

    let contents_by_seq = store
        .get_checkpoint_contents_by_sequence_number(sequence)
        .unwrap()
        .unwrap();
    assert_eq!(contents_by_seq.digest(), contents.digest());

    let by_digest = store
        .get_checkpoint_by_digest(checkpoint.digest())
        .unwrap()
        .unwrap();
    assert_eq!(by_digest.data(), checkpoint.data());

    let contents_by_digest = store
        .get_checkpoint_contents_by_digest(contents.digest())
        .unwrap()
        .unwrap();
    assert_eq!(contents_by_digest.digest(), contents.digest());
}

#[test]
fn test_latest_checkpoint_tracks_highest_sequence() {
    let (_dir, store) = test_store();
    let (low, _) = build_checkpoint(3);
    let (high, _) = build_checkpoint(9);

    // Write out-of-order: the `latest` marker must still resolve to the
    // highest sequence that has been persisted.
    store.write_checkpoint_summary(&high).unwrap();
    store.write_checkpoint_summary(&low).unwrap();

    let highest = store.get_highest_verified_checkpoint().unwrap().unwrap();
    assert_eq!(highest.data().sequence_number, 9);
}

#[test]
fn test_checkpoint_lookups_return_none_when_missing() {
    let (_dir, store) = test_store();
    let (checkpoint, contents) = build_checkpoint(1);

    assert!(
        store
            .get_checkpoint_by_sequence_number(1)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .get_checkpoint_contents_by_sequence_number(1)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .get_checkpoint_by_digest(checkpoint.digest())
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .get_checkpoint_contents_by_digest(contents.digest())
            .unwrap()
            .is_none()
    );
    assert!(store.get_highest_verified_checkpoint().unwrap().is_none());
}
