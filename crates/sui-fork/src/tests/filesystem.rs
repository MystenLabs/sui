// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for [`crate::filesystem::FilesystemStore`]. Wired via
//! `#[cfg(test)] #[path = "tests/filesystem.rs"] mod tests;` so the file
//! lives under `src/tests/` but remains a child of the `filesystem` module
//! and has full `super::*` access to crate-private items.

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::MoveObject;
use sui_types::object::Object;
use sui_types::object::ObjectInner;
use sui_types::object::Owner;

use crate::seed::SeedEntry;
use crate::seed::SeedManifest;

use super::*;

fn test_store() -> (tempfile::TempDir, FilesystemStore) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = FilesystemStore::new_with_root(dir.path().to_path_buf());
    (dir, store)
}

#[test]
fn explicit_data_dir_is_used_as_store_root() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let root = dir.path().join("fork-root");

    let store = FilesystemStore::new(&crate::Node::Mainnet, 42, Some(root.clone())).unwrap();

    assert_eq!(store.root, root);
    assert_eq!(store.objects_dir(), root.join(OBJECTS_DIR));
    assert_eq!(store.checkpoints_dir(), root.join(CHECKPOINTS_DIR));
    assert_eq!(store.transactions_dir(), root.join(TRANSACTIONS_DIR));
    assert_eq!(store.seed_manifest_path(), root.join(SEED_MANIFEST_FILE));
}

#[test]
fn default_root_appends_network_and_checkpoint_to_base_path() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let base = dir.path().join(DATA_DIR);

    let root = FilesystemStore::root_from_base(base.clone(), &crate::Node::Testnet, 99);

    assert_eq!(root, base.join("testnet").join("forked_at_99"));
}

#[cfg(unix)]
fn env_value(vars: &[(&str, &str)], key: &str) -> Option<OsString> {
    vars.iter()
        .find_map(|(name, value)| (*name == key).then(|| OsString::from(*value)))
}

#[cfg(unix)]
#[test]
fn sui_fork_data_env_takes_precedence_over_xdg_and_home() {
    let vars = [
        (SUI_FORK_DATA_ENV, "/tmp/custom-fork-base"),
        ("XDG_DATA_HOME", "/tmp/xdg-data"),
        ("HOME", "/tmp/home"),
    ];

    let base = FilesystemStore::base_path_from_env(|key| env_value(&vars, key)).unwrap();

    assert_eq!(base, PathBuf::from("/tmp/custom-fork-base"));
}

#[cfg(unix)]
#[test]
fn xdg_data_home_env_takes_precedence_over_home() {
    let vars = [("XDG_DATA_HOME", "/tmp/xdg-data"), ("HOME", "/tmp/home")];

    let base = FilesystemStore::base_path_from_env(|key| env_value(&vars, key)).unwrap();

    assert_eq!(base, PathBuf::from("/tmp/xdg-data").join(DATA_DIR));
}

#[cfg(unix)]
#[test]
fn home_env_is_used_when_no_override_or_xdg_data_home() {
    let vars = [("HOME", "/tmp/home")];

    let base = FilesystemStore::base_path_from_env(|key| env_value(&vars, key)).unwrap();

    assert_eq!(
        base,
        PathBuf::from("/tmp/home").join(format!(".{}", DATA_DIR))
    );
}

fn make_object(id: ObjectID, version: u64) -> Object {
    make_object_with_owner(id, version, Owner::Immutable)
}

fn make_object_with_owner(id: ObjectID, version: u64, owner: Owner) -> Object {
    let move_obj = MoveObject::new_gas_coin(SequenceNumber::from_u64(version), id, 1_000_000);
    ObjectInner {
        owner,
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

fn object_dir(store: &FilesystemStore, object_id: &ObjectID) -> std::path::PathBuf {
    store.objects_dir().join(object_id.to_string())
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
fn test_deleted_marker_blocks_latest_but_preserves_exact_versions() {
    let (_dir, store) = test_store();
    let id = ObjectID::random();
    let obj = make_object(id, 5);

    store.write_object(&obj).unwrap();
    let object_ref = obj.compute_object_reference();
    store.mark_object_deleted(&object_ref).unwrap();

    assert!(store.is_object_deleted(&id).unwrap());
    assert!(store.get_latest_object(&id).unwrap().is_none());
    let dir = object_dir(&store, &id);
    assert_eq!(
        fs::read_to_string(dir.join(REMOVED_FILE)).unwrap(),
        format!("deleted {} {}\n", object_ref.1.value(), object_ref.2),
    );
    assert!(!dir.join("deleted").exists());
    assert!(!dir.join("wrapped").exists());

    let exact = store.get_object_at_version(&id, 5).unwrap();
    assert_eq!(exact.unwrap(), obj);

    store.clear_object_deleted(&id).unwrap();
    let latest = store.get_latest_object(&id).unwrap();
    assert_eq!(latest.unwrap(), obj);
}

#[test]
fn test_wrapped_marker_blocks_latest_preserves_exact_and_clears_on_write() {
    let (_dir, store) = test_store();
    let id = ObjectID::random();
    let owner = SuiAddress::random_for_testing_only();
    let obj = make_object_with_owner(id, 5, Owner::AddressOwner(owner));

    store.write_object(&obj).unwrap();
    let object_ref = obj.compute_object_reference();
    store.mark_object_wrapped(&object_ref).unwrap();

    assert!(store.is_object_wrapped(&id).unwrap());
    assert!(store.get_latest_object(&id).unwrap().is_none());
    let dir = object_dir(&store, &id);
    assert_eq!(
        fs::read_to_string(dir.join(REMOVED_FILE)).unwrap(),
        format!("wrapped {} {}\n", object_ref.1.value(), object_ref.2),
    );
    assert!(!dir.join("deleted").exists());
    assert!(!dir.join("wrapped").exists());

    let exact = store.get_object_at_version(&id, 5).unwrap();
    assert_eq!(exact.unwrap(), obj);

    let unwrapped = make_object_with_owner(id, 7, Owner::AddressOwner(owner));
    store.write_object(&unwrapped).unwrap();
    store.clear_object_wrapped(&id).unwrap();

    assert!(!store.is_object_wrapped(&id).unwrap());
    let latest = store.get_latest_object(&id).unwrap();
    assert_eq!(latest.unwrap(), unwrapped);
}

#[test]
fn test_clear_wrapped_preserves_deleted_marker() {
    let (_dir, store) = test_store();
    let id = ObjectID::random();
    let obj = make_object(id, 5);

    store.write_object(&obj).unwrap();
    store
        .mark_object_deleted(&obj.compute_object_reference())
        .unwrap();
    store.clear_object_wrapped(&id).unwrap();

    assert!(store.is_object_deleted(&id).unwrap());
    assert!(!store.is_object_wrapped(&id).unwrap());
    assert!(store.get_latest_object(&id).unwrap().is_none());
}

#[test]
fn test_deleted_marker_overwrites_wrapped_marker() {
    let (_dir, store) = test_store();
    let id = ObjectID::random();
    let obj = make_object(id, 5);
    let object_ref = obj.compute_object_reference();

    store.write_object(&obj).unwrap();
    store.mark_object_wrapped(&object_ref).unwrap();
    store.mark_object_deleted(&object_ref).unwrap();

    assert!(store.is_object_deleted(&id).unwrap());
    assert!(!store.is_object_wrapped(&id).unwrap());
    assert_eq!(
        fs::read_to_string(object_dir(&store, &id).join(REMOVED_FILE)).unwrap(),
        format!("deleted {} {}\n", object_ref.1.value(), object_ref.2),
    );
}

#[test]
fn test_owned_object_index_upserts_removes_and_stays_sorted() {
    let (_dir, store) = test_store();
    let owner = SuiAddress::random_for_testing_only();
    let next_owner = SuiAddress::random_for_testing_only();
    let first_id = ObjectID::random();
    let second_id = ObjectID::random();
    let first = make_object_with_owner(first_id, 1, Owner::AddressOwner(owner));
    let second = make_object_with_owner(second_id, 1, Owner::AddressOwner(owner));

    store
        .apply_owned_object_index_updates(&[], [&second, &first])
        .unwrap();
    let entries = store.get_owned_object_entries().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .windows(2)
            .all(|window| window[0].object_id < window[1].object_id)
    );
    assert!(entries.iter().all(|entry| entry.owner == owner));
    assert!(entries.iter().all(|entry| entry.balance == Some(1_000_000)));

    let transferred = make_object_with_owner(first_id, 2, Owner::AddressOwner(next_owner));
    store
        .apply_owned_object_index_updates(&[], [&transferred])
        .unwrap();
    let first_entry = store
        .get_owned_object_entries()
        .unwrap()
        .into_iter()
        .find(|entry| entry.object_id == first_id)
        .unwrap();
    assert_eq!(first_entry.owner, next_owner);
    assert_eq!(first_entry.version, SequenceNumber::from_u64(2));

    store
        .apply_owned_object_index_updates(&[second_id], std::iter::empty::<&Object>())
        .unwrap();
    let entries = store.get_owned_object_entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].object_id, first_id);
}

#[test]
fn test_seed_manifest_round_trips_and_is_immutable() {
    let (_dir, store) = test_store();
    let owner = SuiAddress::random_for_testing_only();
    let object = make_object_with_owner(ObjectID::random(), 1, Owner::AddressOwner(owner));
    let manifest = SeedManifest {
        network: "testnet".to_owned(),
        checkpoint: 42,
        entries: vec![SeedEntry {
            object_id: object.id(),
            version: object.version(),
            digest: object.digest(),
            owner,
            object_type: object.struct_tag().unwrap(),
            balance: object.as_coin_maybe().map(|coin| coin.value()),
        }],
    };

    store.write_seed_manifest(&manifest).unwrap();

    assert!(store.seed_manifest_exists());
    assert_eq!(store.read_seed_manifest().unwrap(), manifest);
    assert!(store.write_seed_manifest(&manifest).is_err());
}

#[test]
fn test_empty_seed_manifest_round_trips() {
    let (_dir, store) = test_store();
    let manifest = SeedManifest {
        network: "mainnet".to_owned(),
        checkpoint: 42,
        entries: Vec::new(),
    };

    store.write_seed_manifest(&manifest).unwrap();

    assert_eq!(store.read_seed_manifest().unwrap(), manifest);
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
