// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::crypto::deterministic_random_account_key;
use sui_types::object::{MoveObject, Object, Owner, OBJECT_START_VERSION};

/// Make a few test gas objects (all with the same owner).
pub fn test_gas_objects() -> Vec<Object> {
    thread_local! {
        static GAS_OBJECTS: Vec<Object> = (0..50)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                let (owner, _) = deterministic_random_account_key();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
    }

    GAS_OBJECTS.with(|v| v.clone())
}

/// Make a test gas objects.
pub fn generate_gas_object() -> Object {
    let gas_object_id = ObjectID::random();
    let (owner, _) = deterministic_random_account_key();
    Object::with_id_owner_for_testing(gas_object_id, owner)
}

pub fn generate_gas_object_with_balance(balance: u64) -> Object {
    let gas_object_id = ObjectID::random();
    let (owner, _) = deterministic_random_account_key();
    Object::with_id_owner_gas_for_testing(gas_object_id, owner, balance)
}

/// Make a few test gas objects (all with the same owner).
pub fn generate_gas_objects_for_testing(count: usize) -> Vec<Object> {
    (0..count)
        .map(|_i| {
            let gas_object_id = ObjectID::random();
            let (owner, _) = deterministic_random_account_key();
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
        .map(|(_, owner)| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect()
}

// TODO: duplicated in consensus_tests
/// make a test shared object.
pub fn test_shared_object() -> Object {
    thread_local! {
        static SHARED_OBJECT_ID: ObjectID = ObjectID::random();
    }

    let obj = MoveObject::new_gas_coin(OBJECT_START_VERSION, SHARED_OBJECT_ID.with(|id| *id), 10);
    let owner = Owner::Shared {
        initial_shared_version: obj.version(),
    };
    Object::new_move(obj, owner, TransactionDigest::genesis())
}
