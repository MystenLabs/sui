// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared test helpers for `sui-forking` unit tests.

use forking_data_store::SetupStore;
use forking_data_store::stores::{FileSystemStore, ForkingStore};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    object::{Object, Owner},
};

pub type TestForkingStore =
    ForkingStore<FileSystemStore, FileSystemStore, FileSystemStore, FileSystemStore>;

pub fn filesystem_store(
    root: &std::path::Path,
    chain_id: &str,
) -> FileSystemStore {
    let store =
        FileSystemStore::new_with_path(forking_data_store::Node::Mainnet, root.to_path_buf())
            .unwrap();
    store
        .setup(Some(chain_id.to_string()))
        .unwrap();
    store
}

pub fn forking_store(root: &std::path::Path, chain_id: &str) -> TestForkingStore {
    ForkingStore::new(
        filesystem_store(root, chain_id),
        filesystem_store(root, chain_id),
        filesystem_store(root, chain_id),
        filesystem_store(root, chain_id),
    )
}

pub fn test_object(object_id: ObjectID, owner: SuiAddress, version: u64) -> Object {
    Object::with_id_owner_version_for_testing(
        object_id,
        SequenceNumber::from_u64(version),
        Owner::AddressOwner(owner),
    )
}
