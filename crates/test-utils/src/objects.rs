// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::test_account_keys;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::object::{MoveObject, Object, Owner, OBJECT_START_VERSION};

/// Make a few test gas objects (all with the same owner).
pub fn test_gas_objects() -> Vec<Object> {
    (0..50)
        .map(|i| {
            let seed = format!("0x444444444444444{i}");
            let gas_object_id = ObjectID::from_hex_literal(&seed).unwrap();
            let (owner, _) = test_account_keys().pop().unwrap();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect()
}

/// Make a test gas objects.
pub fn generate_gas_object() -> Object {
    let gas_object_id = ObjectID::random();
    let (owner, _) = test_account_keys().pop().unwrap();
    Object::with_id_owner_for_testing(gas_object_id, owner)
}

pub fn generate_gas_object_with_balance(balance: u64) -> Object {
    let gas_object_id = ObjectID::random();
    let (owner, _) = test_account_keys().pop().unwrap();
    Object::with_id_owner_gas_for_testing(gas_object_id, owner, balance)
}

/// Make a few test gas objects (all with the same owner).
pub fn generate_gas_objects_for_testing(count: usize) -> Vec<Object> {
    (0..count)
        .map(|_i| {
            let gas_object_id = ObjectID::random();
            let (owner, _) = test_account_keys().pop().unwrap();
            Object::with_id_owner_gas_for_testing(gas_object_id, owner, u64::MAX)
        })
        .collect()
}

/// Make a few test gas objects (all with the same owner).
pub fn generate_gas_objects_with_owner(count: usize, owner: SuiAddress) -> Vec<Object> {
    (0..count)
        .map(|_i| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_gas_for_testing(gas_object_id, owner, u64::MAX)
        })
        .collect()
}

/// Make a few test gas objects with specific owners.
pub fn test_gas_objects_with_owners<O>(owners: O) -> Vec<Object>
where
    O: IntoIterator<Item = SuiAddress>,
{
    owners
        .into_iter()
        .enumerate()
        .map(|(i, owner)| {
            let seed = format!("0x555555555555555{i}");
            let gas_object_id = ObjectID::from_hex_literal(&seed).unwrap();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect()
}

// TODO: duplicated in consensus_tests
/// make a test shared object.
pub fn test_shared_object() -> Object {
    let seed = "0x6666666666666660";
    let shared_object_id = ObjectID::from_hex_literal(seed).unwrap();
    let obj = MoveObject::new_gas_coin(OBJECT_START_VERSION, shared_object_id, 10);
    let owner = Owner::Shared {
        initial_shared_version: obj.version(),
    };
    Object::new_move(obj, owner, TransactionDigest::genesis())
}
