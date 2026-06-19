// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for [`crate::owned_object_index::OwnedObjectIndexStore`]. Wired via
//! `#[cfg(test)] #[path = "tests/owned_object_index.rs"] mod tests;` so the file lives under
//! `src/tests/` but remains a child of the `owned_object_index` module and has full `super::*`
//! access to crate-private items.

use std::collections::BTreeMap;

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

fn initialize_empty_index(store: &OwnedObjectIndexStore) {
    store
        .replace_from_objects(std::iter::empty::<&Object>())
        .expect("empty owned-object index should initialize");
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

fn test_object_id(byte: u8) -> ObjectID {
    ObjectID::from_single_byte(byte)
}

fn test_address(byte: u8) -> SuiAddress {
    SuiAddress::from(test_object_id(byte))
}

fn owned_object_redactions(
    owners: &[(SuiAddress, &'static str)],
    objects: &[(ObjectID, &'static str)],
) -> insta::Settings {
    let owner_labels = owners
        .iter()
        .map(|(owner, label)| (owner.to_string(), *label))
        .collect::<BTreeMap<_, _>>();
    let object_labels = objects
        .iter()
        .map(|(object_id, label)| (object_id.to_string(), *label))
        .collect::<BTreeMap<_, _>>();

    let mut settings = insta::Settings::clone_current();
    settings.add_dynamic_redaction(".**.owner", move |value, _path| {
        labeled_redaction("owner", value, &owner_labels)
    });
    settings.add_dynamic_redaction(".**.object_id", move |value, _path| {
        labeled_redaction("object_id", value, &object_labels)
    });
    settings
}

fn labeled_redaction(
    field: &str,
    value: insta::internals::Content,
    labels: &BTreeMap<String, &'static str>,
) -> insta::internals::Content {
    let value = value
        .as_str()
        .unwrap_or_else(|| panic!("{field} should serialize as a string"));
    let label = labels
        .get(value)
        .unwrap_or_else(|| panic!("unexpected {field}: {value}"));
    insta::internals::Content::from(*label)
}

#[tokio::test]
async fn test_owned_object_index_updates_transfers_and_deletes() {
    let (_dir, store) = test_store();
    let owner = test_address(0x11);
    let next_owner = test_address(0x12);
    let first_id = test_object_id(0x21);
    let second_id = test_object_id(0x22);
    let first = make_gas_object(first_id, 1, Owner::AddressOwner(owner), 1_000_000);
    let second = make_gas_object(second_id, 1, Owner::AddressOwner(owner), 1_000_000);

    let exists_before = store.owned_object_index_exists().unwrap();
    initialize_empty_index(&store);
    let exists_after_initialization = store.owned_object_index_exists().unwrap();

    store
        .apply_owned_object_index_updates(std::iter::empty(), [&second, &first])
        .unwrap();
    let exists_after_insert = store.owned_object_index_exists().unwrap();

    let after_insert = store.scan_owner(owner, None, None).unwrap();
    let second_cursor = after_insert
        .get(1)
        .expect("inserted rows should include a second cursor")
        .clone();

    let cursor_from_second = store.scan_owner(owner, None, Some(second_cursor)).unwrap();

    let next_owner_empty = store.scan_owner(next_owner, None, None).unwrap();

    let transferred = make_gas_object(first_id, 2, Owner::AddressOwner(next_owner), 1_000_000);
    store
        .apply_owned_object_index_updates([&first], [&transferred])
        .unwrap();

    let after_transfer_original_owner = store.scan_owner(owner, None, None).unwrap();
    let after_transfer_next_owner_from_cursor = store
        .scan_owner(next_owner, None, Some(owned_object_info(&transferred)))
        .unwrap();

    store
        .apply_owned_object_index_updates([&second], std::iter::empty())
        .unwrap();
    let after_delete_all = store.get_owned_object_infos().unwrap();

    owned_object_redactions(
        &[(owner, "[owner]"), (next_owner, "[next_owner]")],
        &[(first_id, "[first]"), (second_id, "[second]")],
    )
    .bind(|| {
        insta::assert_json_snapshot!(
            "owned_object_index_updates_transfers_and_deletes",
            serde_json::json!({
                "exists_before": exists_before,
                "exists_after_initialization": exists_after_initialization,
                "exists_after_insert": exists_after_insert,
                "after_insert": after_insert,
                "cursor_from_second": cursor_from_second,
                "next_owner_empty": next_owner_empty,
                "after_transfer_original_owner": after_transfer_original_owner,
                "after_transfer_next_owner_from_cursor": after_transfer_next_owner_from_cursor,
                "after_delete_all": after_delete_all,
            })
        );
    });
}

#[tokio::test]
async fn test_owned_object_index_orders_coin_balances_descending() {
    let (_dir, store) = test_store();
    let owner = test_address(0x31);
    let low_id = test_object_id(0x41);
    let high_id = test_object_id(0x42);
    let low = make_gas_object(low_id, 1, Owner::AddressOwner(owner), 10);
    let high = make_gas_object(high_id, 1, Owner::AddressOwner(owner), 1_000);

    initialize_empty_index(&store);
    store
        .apply_owned_object_index_updates(std::iter::empty(), [&low, &high])
        .unwrap();

    let infos = store
        .scan_owner(owner, Some(&GasCoin::type_()), None)
        .unwrap();

    owned_object_redactions(
        &[(owner, "[owner]")],
        &[(high_id, "[high_balance]"), (low_id, "[low_balance]")],
    )
    .bind(|| {
        insta::assert_json_snapshot!("owned_object_index_orders_coin_balances_descending", infos);
    });
}

#[tokio::test]
async fn test_owned_object_index_filters_exact_and_wildcard_types() {
    let (_dir, store) = test_store();
    let owner = test_address(0x51);
    let gas_id = test_object_id(0x61);
    let custom_id = test_object_id(0x62);
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

    initialize_empty_index(&store);
    store
        .apply_owned_object_index_updates(std::iter::empty(), [&gas, &custom])
        .unwrap();

    let gas_infos = store
        .scan_owner(owner, Some(&GasCoin::type_()), None)
        .unwrap();

    let custom_infos = store.scan_owner(owner, Some(&custom_type), None).unwrap();

    let wildcard_infos = store.scan_owner(owner, Some(&wildcard_coin), None).unwrap();

    let wrong_type = "0x2::clock::Clock"
        .parse::<StructTag>()
        .expect("wrong type should parse");
    let wrong_type_infos = store.scan_owner(owner, Some(&wrong_type), None).unwrap();

    owned_object_redactions(
        &[(owner, "[owner]")],
        &[(gas_id, "[gas]"), (custom_id, "[custom]")],
    )
    .bind(|| {
        insta::assert_json_snapshot!(
            "owned_object_index_filters_exact_and_wildcard_types",
            serde_json::json!({
                "exact_gas": gas_infos,
                "exact_custom": custom_infos,
                "wildcard_coin": wildcard_infos,
                "wrong_type": wrong_type_infos,
            })
        );
    });
}

#[tokio::test]
async fn test_replace_from_objects_clears_previous_rows_and_marks_empty() {
    let (_dir, store) = test_store();
    let owner = test_address(0x71);
    let object_id = test_object_id(0x72);
    let object = make_gas_object(object_id, 1, Owner::AddressOwner(owner), 1);

    store.replace_from_objects([&object]).unwrap();
    let exists_after_populated = store.owned_object_index_exists().unwrap();
    let populated = store.get_owned_object_infos().unwrap();

    store
        .replace_from_objects(std::iter::empty::<&Object>())
        .unwrap();
    let exists_after_empty = store.owned_object_index_exists().unwrap();
    let empty = store.get_owned_object_infos().unwrap();

    owned_object_redactions(&[(owner, "[owner]")], &[(object_id, "[object]")]).bind(|| {
        insta::assert_json_snapshot!(
            "replace_from_objects_clears_previous_rows_and_marks_empty",
            serde_json::json!({
                "exists_after_populated": exists_after_populated,
                "populated": populated,
                "exists_after_empty": exists_after_empty,
                "empty": empty,
            })
        );
    });
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
