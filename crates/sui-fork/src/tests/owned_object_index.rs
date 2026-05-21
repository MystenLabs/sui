// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for [`crate::owned_object_index::OwnedObjectIndexStore`]. Wired via
//! `#[cfg(test)] #[path = "tests/owned_object_index.rs"] mod tests;` so the file lives under
//! `src/tests/` but remains a child of the `owned_object_index` module and has full `super::*`
//! access to crate-private items.

use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::object::MoveObject;
use sui_types::object::Object;
use sui_types::object::ObjectInner;
use sui_types::object::Owner;

use super::*;

/// Open an [`OwnedObjectIndexStore`] backed by a fresh tempdir.
fn test_store() -> (tempfile::TempDir, OwnedObjectIndexStore) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = OwnedObjectIndexStore::open(dir.path());
    (dir, store)
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

#[test]
fn test_owned_object_index_upserts_removes_and_stays_sorted() {
    let (_dir, store) = test_store();
    let owner = SuiAddress::random_for_testing_only();
    let next_owner = SuiAddress::random_for_testing_only();
    let first_id = ObjectID::random();
    let second_id = ObjectID::random();
    let first = make_object_with_owner(first_id, 1, Owner::AddressOwner(owner));
    let second = make_object_with_owner(second_id, 1, Owner::AddressOwner(owner));

    assert!(!store.owned_object_index_exists().unwrap());
    store
        .apply_owned_object_index_updates(&[], [&second, &first])
        .unwrap();
    assert!(store.owned_object_index_exists().unwrap());

    let entries = store.get_owned_object_entries().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .windows(2)
            .all(|window| window[0].object_ref.0 < window[1].object_ref.0)
    );
    assert!(entries.iter().all(|entry| entry.owner == owner));
    assert!(
        entries
            .iter()
            .all(|entry| entry.object_type == sui_types::gas_coin::GasCoin::type_())
    );
    assert!(entries.iter().all(|entry| entry.balance == Some(1_000_000)));
    assert!(
        entries
            .iter()
            .any(|entry| entry.object_ref == first.compute_object_reference())
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.object_ref == second.compute_object_reference())
    );

    let owner_entries = store
        .get_owned_object_entries_for_owner(owner, None)
        .unwrap();
    assert_eq!(owner_entries, entries);

    let owner_entries_from_cursor = store
        .get_owned_object_entries_for_owner(owner, Some(entries[1].object_ref.0))
        .unwrap();
    assert_eq!(owner_entries_from_cursor.len(), 1);
    assert_eq!(owner_entries_from_cursor[0], entries[1]);

    let next_owner_entries = store
        .get_owned_object_entries_for_owner(next_owner, None)
        .unwrap();
    assert!(next_owner_entries.is_empty());

    let transferred = make_object_with_owner(first_id, 2, Owner::AddressOwner(next_owner));
    store
        .apply_owned_object_index_updates(&[], [&transferred])
        .unwrap();
    let first_entry = store
        .get_owned_object_entries()
        .unwrap()
        .into_iter()
        .find(|entry| entry.object_ref.0 == first_id)
        .unwrap();
    assert_eq!(first_entry.owner, next_owner);
    assert_eq!(
        first_entry.object_ref,
        transferred.compute_object_reference()
    );
    assert_eq!(
        first_entry.object_type,
        sui_types::gas_coin::GasCoin::type_()
    );
    assert_eq!(first_entry.balance, Some(1_000_000));
    let remaining_owner_entries = store
        .get_owned_object_entries_for_owner(owner, None)
        .unwrap();
    assert_eq!(remaining_owner_entries.len(), 1);
    assert_eq!(remaining_owner_entries[0].object_ref.0, second_id);
    assert_eq!(
        store
            .get_owned_object_entries_for_owner(next_owner, Some(first_id))
            .unwrap()
            .into_iter()
            .map(|entry| entry.object_ref)
            .collect::<Vec<_>>(),
        vec![transferred.compute_object_reference()],
    );

    store
        .apply_owned_object_index_updates(&[second_id], std::iter::empty::<&Object>())
        .unwrap();
    let entries = store.get_owned_object_entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].object_ref.0, first_id);
}
