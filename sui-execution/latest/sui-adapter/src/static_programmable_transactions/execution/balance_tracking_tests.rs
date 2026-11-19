// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::balance_tracking::{ObjectData, add_accumulator_events, finish};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use sui_move_natives::object_runtime::{BalanceTotals, BalanceTracking};
use sui_types::{
    SUI_CLOCK_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, TypeTag,
    accumulator_event::AccumulatorEvent,
    accumulator_root::{AccumulatorObjId, AccumulatorValue as AccumulatorRootValue},
    balance::Balance,
    base_types::{ObjectID, SuiAddress},
    digests::{Digest, TransactionDigest},
    effects::{AccumulatorAddress, AccumulatorOperation, AccumulatorValue, AccumulatorWriteV1},
    execution::ExecutionResultsV2,
    gas_coin::GAS,
    move_package::MovePackage,
    object::{Data, MoveObject, OBJECT_START_VERSION, Object, ObjectPermissions, Owner, Party},
};

fn accumulator_event(
    address: SuiAddress,
    ty: TypeTag,
    operation: AccumulatorOperation,
    value: AccumulatorValue,
) -> AccumulatorEvent {
    // Only `Balance` types have a field ID; the ID is not read by balance tracking.
    let accumulator_obj = AccumulatorRootValue::get_field_id(address, &ty)
        .unwrap_or_else(|_| AccumulatorObjId::new_unchecked(ObjectID::random()));
    AccumulatorEvent::new(
        accumulator_obj,
        AccumulatorWriteV1 {
            address: AccumulatorAddress::new(address, ty),
            operation,
            value,
        },
    )
}

fn balance_event(
    address: SuiAddress,
    operation: AccumulatorOperation,
    amount: u64,
) -> AccumulatorEvent {
    let ty = Balance::type_tag(GAS::type_tag());
    accumulator_event(address, ty, operation, AccumulatorValue::Integer(amount))
}

fn results(written: Vec<Object>, deleted: BTreeSet<ObjectID>) -> ExecutionResultsV2 {
    ExecutionResultsV2 {
        written_objects: written.into_iter().map(|o| (o.id(), o)).collect(),
        modified_objects: BTreeSet::new(),
        created_object_ids: BTreeSet::new(),
        deleted_object_ids: deleted,
        user_events: vec![],
        accumulator_events: vec![],
        settlement_input_sui: 0,
        settlement_output_sui: 0,
    }
}

fn empty_tracking() -> BalanceTracking {
    BalanceTracking {
        before: BalanceTotals::new(),
        after: BalanceTotals::new(),
        addresses: BTreeSet::new(),
    }
}

fn total(totals: &BalanceTotals, owner: SuiAddress, type_: &TypeTag) -> Option<u128> {
    totals.get(&owner)?.get(type_).copied()
}

#[test]
fn raw_splits_and_merges_are_gross_totals() {
    let x = SuiAddress::random_for_testing_only();
    let y = SuiAddress::random_for_testing_only();
    let sui: TypeTag = GAS::type_tag();
    let events = [
        balance_event(x, AccumulatorOperation::Split, 7),
        balance_event(x, AccumulatorOperation::Merge, 7),
        balance_event(x, AccumulatorOperation::Split, u64::MAX),
        balance_event(y, AccumulatorOperation::Merge, 3),
    ];
    let mut tracking = empty_tracking();
    add_accumulator_events(&mut tracking, &events).unwrap();
    assert_eq!(total(&tracking.before, x, &sui), Some(7 + u64::MAX as u128));
    assert_eq!(total(&tracking.after, x, &sui), Some(7));
    assert!(!tracking.before.contains_key(&y));
    assert_eq!(total(&tracking.after, y, &sui), Some(3));
    assert_eq!(tracking.addresses, [x, y].into_iter().collect());
}

#[test]
fn accumulator_overflow_is_an_error() {
    let x = SuiAddress::random_for_testing_only();
    let mut tracking = empty_tracking();
    tracking
        .before
        .entry(x)
        .or_default()
        .insert(Rc::new(GAS::type_tag()), u128::MAX);
    let events = [balance_event(x, AccumulatorOperation::Split, 1)];
    assert!(add_accumulator_events(&mut tracking, &events).is_err());
}

fn object(id: ObjectID, owner: Owner) -> Object {
    let coin = MoveObject::new_gas_coin(OBJECT_START_VERSION, id, 1);
    Object::new_move(coin, owner, TransactionDigest::genesis_marker())
}

fn package(id: ObjectID) -> Object {
    let package = MovePackage::new(
        id,
        OBJECT_START_VERSION,
        BTreeMap::new(),
        u64::MAX,
        vec![],
        BTreeMap::new(),
    )
    .unwrap();
    Object::new_package_from_data(Data::Package(package), TransactionDigest::genesis_marker())
}

/// The involved addresses of a user PTB with these inputs, written objects and loaded runtime
/// objects, other than its sender.
fn involved(
    inputs: Vec<Object>,
    written: Vec<Object>,
    loaded: Vec<ObjectID>,
) -> BTreeSet<SuiAddress> {
    let sender = SuiAddress::random_for_testing_only();
    let results = results(written, BTreeSet::new());
    let objects = ObjectData {
        transaction_addresses: vec![sender],
        inputs: inputs.iter().map(|o| (o.id(), o.owner.clone())).collect(),
        loaded_runtime_objects: loaded.into_iter().collect(),
        contained_uids: BTreeSet::new(),
        generated_ids: BTreeSet::new(),
        results: &results,
    };
    let mut addresses = finish(BalanceTotals::new(), BalanceTotals::new(), objects)
        .unwrap()
        .addresses;
    assert!(addresses.remove(&sender));
    addresses
}

#[test]
fn input_object_ids_are_involved() {
    let id = ObjectID::random();
    let shared = object(
        id,
        Owner::Shared {
            initial_shared_version: OBJECT_START_VERSION,
        },
    );
    assert_eq!(involved(vec![shared], vec![], vec![]), [id.into()].into());
}

#[test]
fn input_object_owners_are_involved() {
    let (a, b, c, parent) = (
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
    );
    let party = Party::new(
        ObjectPermissions::NONE,
        [(b, ObjectPermissions::ALL), (c, ObjectPermissions::ALL)].into(),
    )
    .unwrap();
    let inputs = [
        Owner::AddressOwner(a),
        Owner::ObjectOwner(parent),
        Owner::Party {
            start_version: OBJECT_START_VERSION,
            permissions: party,
        },
        Owner::Immutable,
    ]
    .map(|owner| object(ObjectID::random(), owner));
    let ids: BTreeSet<SuiAddress> = inputs.iter().map(|o| o.id().into()).collect();
    let owners = involved(inputs.into(), vec![], vec![])
        .difference(&ids)
        .copied()
        .collect();
    assert_eq!(BTreeSet::from([a, b, c, parent]), owners);
}

#[test]
fn loaded_child_and_received_ids_are_involved() {
    let (child, received) = (ObjectID::random(), ObjectID::random());
    assert_eq!(
        involved(vec![], vec![], vec![child, received]),
        [child.into(), received.into()].into(),
    );
}

#[test]
fn packages_and_system_objects_are_not_involved() {
    let clock = object(
        SUI_CLOCK_OBJECT_ID,
        Owner::Shared {
            initial_shared_version: OBJECT_START_VERSION,
        },
    );
    let owned_by_framework = object(
        ObjectID::random(),
        Owner::AddressOwner(SUI_FRAMEWORK_ADDRESS.into()),
    );
    let owned_id = owned_by_framework.id();
    let written = vec![package(ObjectID::random())];
    assert_eq!(
        involved(vec![clock, owned_by_framework], written, vec![]),
        [owned_id.into()].into()
    );
}

#[test]
fn non_balance_and_non_integer_events_only_add_addresses() {
    let (x, y) = (
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
    );
    let events = [
        accumulator_event(
            x,
            GAS::type_tag(),
            AccumulatorOperation::Merge,
            AccumulatorValue::Integer(5),
        ),
        accumulator_event(
            y,
            Balance::type_tag(GAS::type_tag()),
            AccumulatorOperation::Merge,
            AccumulatorValue::EventDigest(nonempty::nonempty![(0, Digest::random())]),
        ),
    ];
    let mut tracking = empty_tracking();
    add_accumulator_events(&mut tracking, &events).unwrap();
    assert!(tracking.before.is_empty());
    assert!(tracking.after.is_empty());
    assert_eq!(tracking.addresses, [x, y].into_iter().collect());
}

#[test]
fn sponsor_and_contained_generated_and_deleted_ids_are_involved() {
    let (sender, sponsor) = (
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
    );
    let (contained, generated, deleted) =
        (ObjectID::random(), ObjectID::random(), ObjectID::random());
    let results = results(vec![], [deleted].into());
    let objects = ObjectData {
        transaction_addresses: vec![sender, sponsor],
        inputs: vec![],
        loaded_runtime_objects: BTreeSet::new(),
        // A deleted object must have been read, here as a wrapped UID.
        contained_uids: [contained, deleted].into(),
        generated_ids: [generated].into(),
        results: &results,
    };
    let addresses = finish(BalanceTotals::new(), BalanceTotals::new(), objects)
        .unwrap()
        .addresses;
    assert_eq!(
        addresses,
        [
            sender,
            sponsor,
            contained.into(),
            generated.into(),
            deleted.into()
        ]
        .into()
    );
}

/// Whether `finish` succeeds on a PTB with no inputs that writes `written` and deletes `deleted`.
fn finishes_unread(written: Vec<Object>, deleted: BTreeSet<ObjectID>) -> bool {
    let results = results(written, deleted);
    let objects = ObjectData {
        transaction_addresses: vec![SuiAddress::random_for_testing_only()],
        inputs: vec![],
        loaded_runtime_objects: BTreeSet::new(),
        contained_uids: BTreeSet::new(),
        generated_ids: BTreeSet::new(),
        results: &results,
    };
    finish(BalanceTotals::new(), BalanceTotals::new(), objects).is_ok()
}

#[test]
#[cfg_attr(
    debug_assertions,
    should_panic(expected = "without being read or created")
)]
fn writing_an_unread_object_is_an_error() {
    let owner = Owner::AddressOwner(SuiAddress::random_for_testing_only());
    assert!(!finishes_unread(
        vec![object(ObjectID::random(), owner)],
        BTreeSet::new()
    ));
}

#[test]
#[cfg_attr(
    debug_assertions,
    should_panic(expected = "without being read or created")
)]
fn deleting_an_unread_object_is_an_error() {
    assert!(!finishes_unread(vec![], [ObjectID::random()].into()));
}

#[test]
fn writing_a_package_needs_no_read() {
    assert!(finishes_unread(
        vec![package(ObjectID::random())],
        BTreeSet::new()
    ));
}
