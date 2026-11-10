// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::coin_regulation::{check_address_reads, check_touched_types, touched_coin_types};
use move_binary_format::errors::Location;
use move_core_types::vm_status::StatusCode;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use sui_move_natives::object_runtime::BalanceTracking;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    MoveTypeTagTrait, SUI_FREEZE_LIST_OBJECT_ID, TypeTag,
    base_types::{SequenceNumber, SuiAddress},
    coin_regulation::{
        CoinTypeKey, CoinTypeState, FreezeListField, FrozenAddressKey, FrozenAddressState,
        UnfreezeLock, coin_type_name,
    },
    collection_types::VecSet,
    digests::TransactionDigest,
    error::{ExecutionError, VMMemoryLimitExceededSubStatusCode},
    execution_status::ExecutionErrorKind,
    gas_coin::GAS,
    in_memory_storage::InMemoryStorage,
    metrics::ExecutionMetrics,
    object::{Object, Owner},
    parse_sui_type_tag,
};

const VERSION: SequenceNumber = SequenceNumber::from_u64(7);

fn coin(name: &str) -> TypeTag {
    parse_sui_type_tag(&format!("0x123::{}::{name}", name.to_lowercase())).unwrap()
}

fn field<K, V>(field: FreezeListField<K>, value: V) -> Object
where
    K: Serialize + DeserializeOwned + std::fmt::Debug,
    V: MoveTypeTagTrait + Serialize + DeserializeOwned,
{
    let field = field
        .into_field(value)
        .unwrap()
        .into_move_object_unsafe_for_testing(VERSION)
        .unwrap();
    Object::new_move(
        field,
        Owner::ObjectOwner(SUI_FREEZE_LIST_OBJECT_ID.into()),
        TransactionDigest::genesis_marker(),
    )
}

/// A FreezeList with `T` registered, `paused` as given, and `frozen` addresses frozen.
fn regulate(objects: &mut Vec<Object>, coin_type: &TypeTag, paused: bool, frozen: &[SuiAddress]) {
    objects.push(field(
        CoinTypeKey::field(coin_type),
        CoinTypeState {
            paused,
            num_frozen_addresses: frozen.len() as u64,
        },
    ));
    for addr in frozen {
        objects.push(field(
            FrozenAddressKey::field(coin_type, *addr),
            FrozenAddressState {
                seize_auths: VecSet { contents: vec![] },
                frozen_epoch: 0,
                unfreeze_lock: UnfreezeLock::None,
            },
        ));
    }
}

fn check(
    store: &InMemoryStorage,
    touched: &[&TypeTag],
    addresses: &[SuiAddress],
) -> Result<(), ExecutionError> {
    check_touched_types(
        store,
        VERSION,
        &touched.iter().copied().collect(),
        &addresses.iter().copied().collect(),
        |_| Ok::<_, ExecutionError>(()),
    )
}

fn assert_paused(result: Result<(), ExecutionError>, coin_type: &TypeTag) {
    assert_eq!(
        result.unwrap_err().kind(),
        &ExecutionErrorKind::CoinTypeGlobalPause {
            coin_type: coin_type_name(coin_type),
        }
    );
}

fn assert_denied(result: Result<(), ExecutionError>, address: SuiAddress, coin_type: &TypeTag) {
    assert_eq!(
        result.unwrap_err().kind(),
        &ExecutionErrorKind::AddressDeniedForCoin {
            address,
            coin_type: coin_type_name(coin_type),
        }
    );
}

#[test]
fn touched_types_ignore_sui_and_zero_totals() {
    let x = SuiAddress::random_for_testing_only();
    let y = SuiAddress::random_for_testing_only();
    let (a, b, c, d) = (coin("A"), coin("B"), coin("C"), coin("D"));
    let totals = |entries: Vec<(TypeTag, u128)>| {
        entries
            .into_iter()
            .map(|(type_, total)| (Rc::new(type_), total))
            .collect::<BTreeMap<_, _>>()
    };
    let tracking = BalanceTracking {
        before: BTreeMap::from([(
            x,
            totals(vec![(GAS::type_tag(), 10), (a.clone(), 1), (c.clone(), 0)]),
        )]),
        after: BTreeMap::from([
            (y, totals(vec![(b.clone(), 2), (d.clone(), 0)])),
            (x, totals(vec![(a.clone(), 0)])),
        ]),
        addresses: BTreeSet::new(),
    };
    assert_eq!(touched_coin_types(&tracking), BTreeSet::from([&a, &b]));
}

#[test]
fn unregulated_types_pass() {
    let x = SuiAddress::random_for_testing_only();
    let (a, b) = (coin("A"), coin("B"));
    let mut objects = vec![];
    regulate(&mut objects, &b, true, &[x]);
    let store = InMemoryStorage::new(objects);
    check(&store, &[&a], &[x]).unwrap();
}

#[test]
fn paused_type_fails() {
    let x = SuiAddress::random_for_testing_only();
    let a = coin("A");
    let mut objects = vec![];
    regulate(&mut objects, &a, true, &[]);
    let store = InMemoryStorage::new(objects);
    assert_paused(check(&store, &[&a], &[x]), &a);
}

#[test]
fn pause_is_checked_before_freeze() {
    let x = SuiAddress::random_for_testing_only();
    let (a, b) = (coin("A"), coin("B"));
    let mut objects = vec![];
    regulate(&mut objects, &a, false, &[x]);
    regulate(&mut objects, &b, true, &[]);
    let store = InMemoryStorage::new(objects);
    assert_paused(check(&store, &[&a, &b], &[x]), &b);
}

#[test]
fn frozen_involved_address_fails() {
    let x = SuiAddress::random_for_testing_only();
    let y = SuiAddress::random_for_testing_only();
    let z = SuiAddress::random_for_testing_only();
    let (a, b) = (coin("A"), coin("B"));
    let mut objects = vec![];
    regulate(&mut objects, &a, false, &[x]);
    regulate(&mut objects, &b, false, &[y]);
    let store = InMemoryStorage::new(objects);
    assert_denied(check(&store, &[&a], &[x, z]), x, &a);
    // Frozen for another type, or not involved.
    check(&store, &[&a], &[y, z]).unwrap();
    check(&store, &[&b], &[x, z]).unwrap();
}

#[test]
fn no_frozen_addresses_skips_address_reads() {
    let x = SuiAddress::random_for_testing_only();
    let a = coin("A");
    let mut objects = vec![];
    regulate(&mut objects, &a, false, &[x]);
    // A stale count of zero means the address fields are never read.
    *objects.first_mut().unwrap() = field(
        CoinTypeKey::field(&a),
        CoinTypeState {
            paused: false,
            num_frozen_addresses: 0,
        },
    );
    let store = InMemoryStorage::new(objects);
    check(&store, &[&a], &[x]).unwrap();
}

#[test]
fn address_reads_are_counted_before_any_is_made() {
    let (x, y, z) = (
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
    );
    let (a, b, c) = (coin("A"), coin("B"), coin("C"));
    let mut objects = vec![];
    regulate(&mut objects, &a, false, &[x]);
    regulate(&mut objects, &b, false, &[y]);
    regulate(&mut objects, &c, false, &[]);
    let store = InMemoryStorage::new(objects);
    let mut requested = None;
    let result = check_touched_types(
        &store,
        VERSION,
        &[&a, &b, &c].into_iter().collect(),
        &[x, y, z].into_iter().collect(),
        |reads| {
            requested = Some(reads);
            Err(ExecutionError::invariant_violation("over the limit"))
        },
    );
    // Only the two types with frozen addresses are read, for each of the three addresses.
    assert_eq!(requested, Some(6));
    assert_eq!(
        result.unwrap_err().kind(),
        &ExecutionErrorKind::InvariantViolation
    );
}

#[test]
fn address_reads_fail_at_the_limit() {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
    config.set_max_freeze_list_address_reads_for_testing(6);
    let metrics = ExecutionMetrics::new(&prometheus::Registry::new());
    check_address_reads(&config, &metrics.limits_metrics, 5).unwrap();
    let err = check_address_reads(&config, &metrics.limits_metrics, 6)
        .unwrap_err()
        .finish(Location::Undefined);
    assert_eq!(err.major_status(), StatusCode::MEMORY_LIMIT_EXCEEDED);
    assert_eq!(
        err.sub_status(),
        Some(VMMemoryLimitExceededSubStatusCode::FREEZE_LIST_ADDRESS_READ_LIMIT_EXCEEDED as u64)
    );
}
