// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for [`crate::metadata::ForkMetadataStore`]. Wired via
//! `#[path]` in `metadata.rs`, so `super::*` resolves into that module.

use std::collections::BTreeMap;
use std::ffi::OsString;

use sui_types::base_types::ObjectID;

use super::*;

fn env_value(vars: &BTreeMap<&'static str, &'static str>, key: &str) -> Option<OsString> {
    vars.get(key).map(OsString::from)
}

#[test]
fn explicit_data_dir_is_used_as_store_root() {
    let root = tempfile::tempdir().expect("tempdir");
    let store = ForkMetadataStore::new(&crate::Node::Mainnet, 42, Some(root.path().to_path_buf()))
        .expect("store should construct");

    assert_eq!(store.root(), root.path());
    assert_eq!(
        store.seed_manifest_path(),
        root.path().join(SEED_MANIFEST_FILE)
    );
}

#[test]
fn default_root_appends_network_and_checkpoint_to_base_path() {
    let base = std::path::PathBuf::from("/tmp/sui-fork-test");
    let root = ForkMetadataStore::root_from_base(base.clone(), &crate::Node::Testnet, 99);

    assert_eq!(root, base.join("testnet").join("forked_at_99"));
}

#[test]
fn sui_fork_data_env_takes_precedence_over_xdg_and_home() {
    let vars = BTreeMap::from([
        ("SUI_FORK_DATA", "/custom/fork-root"),
        ("XDG_DATA_HOME", "/xdg"),
        ("HOME", "/home/alice"),
    ]);

    let base = ForkMetadataStore::base_path_from_env(|key| env_value(&vars, key)).unwrap();

    assert_eq!(base, std::path::PathBuf::from("/custom/fork-root"));
}

#[cfg(unix)]
#[test]
fn xdg_data_home_env_takes_precedence_over_home() {
    let vars = BTreeMap::from([("XDG_DATA_HOME", "/xdg"), ("HOME", "/home/alice")]);

    let base = ForkMetadataStore::base_path_from_env(|key| env_value(&vars, key)).unwrap();

    assert_eq!(base, std::path::PathBuf::from("/xdg").join(DATA_DIR));
}

#[cfg(unix)]
#[test]
fn home_env_is_used_when_no_override_or_xdg_data_home() {
    let vars = BTreeMap::from([("HOME", "/home/alice")]);

    let base = ForkMetadataStore::base_path_from_env(|key| env_value(&vars, key)).unwrap();

    assert_eq!(base, std::path::PathBuf::from("/home/alice/.sui_fork_data"));
}

#[test]
fn seed_manifest_round_trips_and_is_immutable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = ForkMetadataStore::new_with_root(dir.path().to_path_buf());
    let manifest = SeedManifest {
        network: "custom".to_owned(),
        checkpoint: 42,
        addresses: Vec::new(),
        entries: Vec::new(),
    };

    store.write_seed_manifest(&manifest).unwrap();

    assert!(store.seed_manifest_exists());
    assert_eq!(store.read_seed_manifest().unwrap(), manifest);
    assert!(store.write_seed_manifest(&manifest).is_err());
}

#[test]
fn inventory_metadata_tracks_remote_completion_sets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = ForkMetadataStore::new_with_root(dir.path().to_path_buf());
    let parent = ObjectID::random();
    let type_filter = "0x2::coin::Coin<0x2::sui::SUI>";

    assert!(!store.object_owner_inventory_complete(parent).unwrap());
    assert!(!store.type_inventory_complete(type_filter).unwrap());

    store.mark_object_owner_inventory_complete(parent).unwrap();
    store.mark_type_inventory_complete(type_filter).unwrap();

    assert!(store.object_owner_inventory_complete(parent).unwrap());
    assert!(store.type_inventory_complete(type_filter).unwrap());
}

#[test]
fn inventory_metadata_accepts_older_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = ForkMetadataStore::new_with_root(dir.path().to_path_buf());
    let parent = ObjectID::random();
    fs::write(
        store.inventory_metadata_path(),
        serde_json::json!({
            "completed_object_owners": [parent],
        })
        .to_string(),
    )
    .unwrap();

    assert!(store.object_owner_inventory_complete(parent).unwrap());
    assert!(!store.type_inventory_complete("0x2::clock::Clock").unwrap());
}
