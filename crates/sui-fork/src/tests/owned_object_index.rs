// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for [`crate::owned_object_index::OwnedObjectIndexStore`]. Wired via
//! `#[cfg(test)] #[path = "tests/owned_object_index.rs"] mod tests;` so the file lives under
//! `src/tests/` but remains a child of the `owned_object_index` module and has full `super::*`
//! access to crate-private items.

use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::gas_coin::GasCoin;
use sui_types::object::MoveObject;
use sui_types::object::Object;
use sui_types::object::ObjectInner;
use sui_types::object::Owner;
use sui_types::storage::OwnedObjectInfo;

use super::*;

/// Open an [`OwnedObjectIndexStore`] backed by a fresh tempdir.
fn test_store() -> (tempfile::TempDir, OwnedObjectIndexStore) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let store = OwnedObjectIndexStore::open(dir.path());
    (dir, store)
}

fn make_gas_object(id: ObjectID, version: u64, owner: Owner, value: u64) -> Object {
    make_move_object(
        MoveObject::new_gas_coin(SequenceNumber::from_u64(version), id, value),
        owner,
    )
}

fn make_coin_object(
    id: ObjectID,
    version: u64,
    owner: Owner,
    coin_type: TypeTag,
    value: u64,
) -> Object {
    make_move_object(
        MoveObject::new_coin(coin_type, SequenceNumber::from_u64(version), id, value),
        owner,
    )
}

fn make_move_object(move_obj: MoveObject, owner: Owner) -> Object {
    ObjectInner {
        owner,
        data: sui_types::object::Data::Move(move_obj),
        previous_transaction: TransactionDigest::genesis_marker(),
        storage_rebate: 0,
    }
    .into()
}

fn custom_coin_type() -> TypeTag {
    TypeTag::Struct(Box::new(
        "0x42::custom_coin::CUSTOM"
            .parse::<StructTag>()
            .expect("custom coin type should parse"),
    ))
}

macro_rules! assert_same_info {
    ($actual:expr, $expected:expr $(,)?) => {{
        let actual = $actual;
        let expected = $expected;
        assert_eq!(actual.owner, expected.owner);
        assert_eq!(actual.object_type, expected.object_type);
        assert_eq!(actual.balance, expected.balance);
        assert_eq!(actual.object_id, expected.object_id);
        assert_eq!(actual.version, expected.version);
    }};
}

macro_rules! assert_info_matches_object {
    ($info:expr, $object:expr $(,)?) => {{
        assert_same_info!($info, &owned_object_info($object));
    }};
}

#[tokio::test]
async fn test_owned_object_index_updates_transfers_and_deletes() {
    let (_dir, store) = test_store();
    let owner = SuiAddress::random_for_testing_only();
    let next_owner = SuiAddress::random_for_testing_only();
    let first_id = ObjectID::random();
    let second_id = ObjectID::random();
    let first = make_gas_object(first_id, 1, Owner::AddressOwner(owner), 1_000_000);
    let second = make_gas_object(second_id, 1, Owner::AddressOwner(owner), 1_000_000);

    assert!(!store.owned_object_index_exists().unwrap());
    store
        .apply_owned_object_index_updates(std::iter::empty(), [&second, &first])
        .unwrap();
    assert!(store.owned_object_index_exists().unwrap());

    let infos = store.scan_owner(owner, None, None).unwrap();
    assert_eq!(infos.len(), 2);
    assert!(infos[0].object_id < infos[1].object_id);
    assert!(infos.iter().all(|info| info.owner == owner));
    assert!(
        infos
            .iter()
            .all(|info| info.object_type == GasCoin::type_())
    );
    assert!(infos.iter().all(|info| info.balance == Some(1_000_000)));

    let infos_from_cursor = store
        .scan_owner(owner, None, Some(infos[1].clone()))
        .unwrap();
    assert_eq!(infos_from_cursor.len(), 1);
    assert_same_info!(&infos_from_cursor[0], &infos[1]);

    assert!(store.scan_owner(next_owner, None, None).unwrap().is_empty());

    let transferred = make_gas_object(first_id, 2, Owner::AddressOwner(next_owner), 1_000_000);
    store
        .apply_owned_object_index_updates([&first], [&transferred])
        .unwrap();

    let remaining_owner_infos = store.scan_owner(owner, None, None).unwrap();
    assert_eq!(remaining_owner_infos.len(), 1);
    assert_eq!(remaining_owner_infos[0].object_id, second_id);

    let next_owner_infos = store
        .scan_owner(next_owner, None, Some(owned_object_info(&transferred)))
        .unwrap();
    assert_eq!(next_owner_infos.len(), 1);
    assert_info_matches_object!(&next_owner_infos[0], &transferred);

    store
        .apply_owned_object_index_updates([&second], std::iter::empty())
        .unwrap();
    let infos = store.get_owned_object_infos().unwrap();
    assert_eq!(infos.len(), 1);
    assert_info_matches_object!(&infos[0], &transferred);
}

#[tokio::test]
async fn test_owned_object_index_orders_coin_balances_descending() {
    let (_dir, store) = test_store();
    let owner = SuiAddress::random_for_testing_only();
    let low_id = ObjectID::random();
    let high_id = ObjectID::random();
    let low = make_gas_object(low_id, 1, Owner::AddressOwner(owner), 10);
    let high = make_gas_object(high_id, 1, Owner::AddressOwner(owner), 1_000);

    store
        .apply_owned_object_index_updates(std::iter::empty(), [&low, &high])
        .unwrap();

    let infos = store
        .scan_owner(owner, Some(&GasCoin::type_()), None)
        .unwrap();
    assert_eq!(
        infos
            .into_iter()
            .map(|info| (info.object_id, info.balance))
            .collect::<Vec<_>>(),
        vec![(high_id, Some(1_000)), (low_id, Some(10))],
    );
}

#[tokio::test]
async fn test_owned_object_index_filters_exact_and_wildcard_types() {
    let (_dir, store) = test_store();
    let owner = SuiAddress::random_for_testing_only();
    let gas_id = ObjectID::random();
    let custom_id = ObjectID::random();
    let gas = make_gas_object(gas_id, 1, Owner::AddressOwner(owner), 1_000_000);
    let custom = make_coin_object(
        custom_id,
        1,
        Owner::AddressOwner(owner),
        custom_coin_type(),
        7,
    );
    let custom_type = custom.struct_tag().expect("custom coin should have a type");
    let wildcard_coin = "0x2::coin::Coin"
        .parse::<StructTag>()
        .expect("wildcard coin type should parse");

    store
        .apply_owned_object_index_updates(std::iter::empty(), [&gas, &custom])
        .unwrap();

    let gas_infos = store
        .scan_owner(owner, Some(&GasCoin::type_()), None)
        .unwrap();
    assert_eq!(gas_infos.len(), 1);
    assert_info_matches_object!(&gas_infos[0], &gas);

    let custom_infos = store.scan_owner(owner, Some(&custom_type), None).unwrap();
    assert_eq!(custom_infos.len(), 1);
    assert_info_matches_object!(&custom_infos[0], &custom);

    let wildcard_infos = store
        .scan_owner(owner, Some(&wildcard_coin), None)
        .unwrap()
        .into_iter()
        .map(|info| info.object_id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(wildcard_infos, [gas_id, custom_id].into_iter().collect(),);

    let wrong_type = "0x2::clock::Clock"
        .parse::<StructTag>()
        .expect("wrong type should parse");
    assert!(
        store
            .scan_owner(owner, Some(&wrong_type), None)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn test_replace_from_objects_clears_previous_rows_and_marks_empty() {
    let (_dir, store) = test_store();
    let owner = SuiAddress::random_for_testing_only();
    let object = make_gas_object(ObjectID::random(), 1, Owner::AddressOwner(owner), 1);

    store.replace_from_objects([&object]).unwrap();
    assert_eq!(store.get_owned_object_infos().unwrap().len(), 1);

    store
        .replace_from_objects(std::iter::empty::<&Object>())
        .unwrap();
    assert!(store.owned_object_index_exists().unwrap());
    assert!(store.get_owned_object_infos().unwrap().is_empty());
}

fn owned_object_info(object: &Object) -> OwnedObjectInfo {
    let owner = match object.owner() {
        Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => *owner,
        _ => panic!("test object should be address-owned"),
    };
    OwnedObjectInfo {
        owner,
        object_type: object.struct_tag().expect("test object should have a type"),
        balance: object.as_coin_maybe().map(|coin| coin.value()),
        object_id: object.id(),
        version: object.version(),
    }
}
