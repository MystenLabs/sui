// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::balance::Balance;
use crate::base_types::MoveObjectType;
use crate::digests::TransactionDigest;
use crate::gas_coin::GAS;
use crate::in_memory_storage::InMemoryStorage;
use crate::object::{MoveObject, Owner};
use crate::storage::{BackingPackageStore, PackageObject};
use std::collections::BTreeMap;

fn alice() -> SuiAddress {
    id("0xa11ce").into()
}

fn balance_of_sui() -> TypeTag {
    Balance::type_tag(GAS::type_tag())
}

fn id(hex: &str) -> ObjectID {
    ObjectID::from_hex_literal(hex).unwrap()
}

fn field_object<K, V>(field: FreezeListField<K>, value: V, version: SequenceNumber) -> Object
where
    K: Serialize + DeserializeOwned + std::fmt::Debug,
    V: MoveTypeTagTrait + Serialize + DeserializeOwned,
{
    let field = field
        .into_field(value)
        .unwrap()
        .into_move_object_unsafe_for_testing(version)
        .unwrap();
    Object::new_move(
        field,
        Owner::ObjectOwner(SUI_FREEZE_LIST_OBJECT_ID.into()),
        TransactionDigest::genesis_marker(),
    )
}

fn move_object(type_: StructTag, contents: Vec<u8>) -> Object {
    let object = unsafe {
        MoveObject::new_from_execution_with_limit(
            MoveObjectType::from(type_),
            false,
            SequenceNumber::from_u64(1),
            contents,
            256,
        )
    }
    .unwrap();
    Object::new_move(
        object,
        Owner::AddressOwner(alice()),
        TransactionDigest::genesis_marker(),
    )
}

/// The expected IDs were printed by a Move unit test calling `dynamic_field::hash_type_and_key`
/// on 0xcf with keys built by `coin_regulation` itself.
#[test]
fn field_ids_match_move() {
    let cases = [
        (
            CoinTypeKey::field_id(&GAS::type_tag()),
            "0x5403417134c19ca10f3e25c87f66b61e241d7471e447bc7334559ce79538020f",
        ),
        (
            CoinTypeKey::field_id(&balance_of_sui()),
            "0x295c166562811eb6c40c7d9d2455104541071c60e0b1c86db15a4585b4d2a2db",
        ),
        (
            FrozenAddressKey::field_id(&GAS::type_tag(), alice()),
            "0x3dabde2515651a6408d9169ab2e7c44154317e0ee5a24afdec2e01faadd39b19",
        ),
        (
            FrozenAddressKey::field_id(&balance_of_sui(), alice()),
            "0x6a9a31dc32650dbb63cf4ff992edb1aea81462a54709bd50b10296899dbc109b",
        ),
    ];
    for (actual, expected) in cases {
        assert_eq!(actual.unwrap(), id(expected));
    }
}

#[test]
fn coin_type_key_is_one_false_byte() {
    assert_eq!(bcs::to_bytes(&CoinTypeKey::default()).unwrap(), vec![0]);
}

#[test]
fn coin_type_name_uses_full_addresses_without_prefix() {
    assert_eq!(
        coin_type_name(&balance_of_sui()),
        "0000000000000000000000000000000000000000000000000000000000000002::balance::Balance\
         <0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>",
    );
}

#[test]
fn bounded_and_latest_reads() {
    let version = SequenceNumber::from_u64(5);
    let regulated = GAS::type_tag();
    let unregulated = balance_of_sui();
    let bob = SuiAddress::from(id("0xb0b"));
    let state = CoinTypeState {
        paused: true,
        num_frozen_addresses: 1,
    };
    let frozen = FrozenAddressState {
        seize_auths: VecSet {
            contents: vec![id("0x5e12e")],
        },
        frozen_epoch: 3,
        unfreeze_lock: UnfreezeLock::None,
    };
    let store = InMemoryStorage::new(vec![
        field_object(CoinTypeKey::field(&regulated), state.clone(), version),
        field_object(
            FrozenAddressKey::field(&regulated, alice()),
            frozen.clone(),
            version,
        ),
    ]);

    assert_eq!(
        get_coin_type_state(&store, version, &regulated).unwrap(),
        Some(state.clone())
    );
    assert_eq!(
        get_coin_type_state(&store, version, &unregulated).unwrap(),
        None
    );
    assert_eq!(
        get_frozen_address_state(&store, version, &regulated, alice()).unwrap(),
        Some(frozen.clone())
    );
    assert_eq!(
        get_frozen_address_state(&store, version, &regulated, bob).unwrap(),
        None
    );
    assert_eq!(
        get_frozen_address_state(&store, version, &unregulated, alice()).unwrap(),
        None
    );

    assert_eq!(
        get_latest_coin_type_state(&store, &regulated).unwrap(),
        Some(state)
    );
    assert_eq!(
        get_latest_coin_type_state(&store, &unregulated).unwrap(),
        None
    );
    assert_eq!(
        get_latest_frozen_address_state(&store, &regulated, alice()).unwrap(),
        Some(frozen)
    );
    assert_eq!(
        get_latest_frozen_address_state(&store, &regulated, bob).unwrap(),
        None
    );
}

/// Keeps every version of each child.
#[derive(Default)]
struct VersionedChildren(BTreeMap<ObjectID, Vec<Object>>);

impl VersionedChildren {
    fn insert(&mut self, object: Object) {
        self.0.entry(object.id()).or_default().push(object);
    }
}

impl BackingPackageStore for VersionedChildren {
    fn get_package_object(&self, _package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        Ok(None)
    }
}

impl RuntimeObjectResolver for VersionedChildren {
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.0.get(child).and_then(|versions| {
            versions
                .iter()
                .filter(|object| object.version() <= child_version_upper_bound)
                .max_by_key(|object| object.version())
                .cloned()
        }))
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        unreachable!()
    }
}

#[test]
fn bounded_reads_ignore_later_versions() {
    let coin_type = GAS::type_tag();
    let state = |paused| CoinTypeState {
        paused,
        num_frozen_addresses: 1,
    };
    let frozen = |frozen_epoch| FrozenAddressState {
        seize_auths: VecSet { contents: vec![] },
        frozen_epoch,
        unfreeze_lock: UnfreezeLock::None,
    };
    let (v3, v5, v7) = (
        SequenceNumber::from_u64(3),
        SequenceNumber::from_u64(5),
        SequenceNumber::from_u64(7),
    );
    let mut store = VersionedChildren::default();
    store.insert(field_object(
        CoinTypeKey::field(&coin_type),
        state(false),
        v3,
    ));
    store.insert(field_object(
        CoinTypeKey::field(&coin_type),
        state(true),
        v7,
    ));
    store.insert(field_object(
        FrozenAddressKey::field(&coin_type, alice()),
        frozen(7),
        v7,
    ));

    let read_state = |version| get_coin_type_state(&store, version, &coin_type).unwrap();
    let read_frozen =
        |version| get_frozen_address_state(&store, version, &coin_type, alice()).unwrap();
    assert_eq!(read_state(SequenceNumber::from_u64(2)), None);
    assert_eq!(read_state(v5), Some(state(false)));
    assert_eq!(read_state(v7), Some(state(true)));
    assert_eq!(read_frozen(v5), None);
    assert_eq!(read_frozen(v7), Some(frozen(7)));
}

#[test]
fn parse_seize_auth_objects() {
    let auth = SeizeAuth {
        id: UID::new(id("0x5e12e")),
        frozen_address: alice(),
        frozen_epoch: 3,
    };
    let contents = bcs::to_bytes(&auth).unwrap();

    let object = move_object(SeizeAuth::type_(balance_of_sui()), contents.clone());
    assert_eq!(
        parse_seize_auth(&object),
        Some((balance_of_sui(), alice(), 3))
    );

    let cap = coin_regulation_struct(ident_str!("ComplianceCap"), vec![GAS::type_tag()]);
    assert_eq!(parse_seize_auth(&move_object(cap, contents.clone())), None);

    let mut elsewhere = SeizeAuth::type_(GAS::type_tag());
    elsewhere.address = id("0x123").into();
    assert_eq!(parse_seize_auth(&move_object(elsewhere, contents)), None);

    let coin = Object::new_move(
        MoveObject::new_coin(GAS::type_tag(), SequenceNumber::from_u64(1), id("0xc0"), 10),
        Owner::AddressOwner(alice()),
        TransactionDigest::genesis_marker(),
    );
    assert_eq!(parse_seize_auth(&coin), None);
}
