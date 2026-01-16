// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator_metadata;

use sui::accumulator::AccumulatorRoot;
use sui::bag;
use sui::dynamic_field;

const EInvariantViolation: u64 = 0;

/// === Accumulator metadata ===
///
/// Metadata system has been removed, but structs must remain for backwards compatibility.

#[allow(unused_field)]
public struct OwnerKey has copy, drop, store {
    owner: address,
}

/// An owner field, to which all AccumulatorMetadata fields for the owner are
/// attached.
#[allow(unused_field)]
public struct Owner has store {
    /// The individual balances owned by the owner.
    balances: bag::Bag,
    owner: address,
}

public struct MetadataKey<phantom T>() has copy, drop, store;

/// A metadata field for a balance field with type T.
#[allow(unused_field)]
public struct Metadata<phantom T> has store {
    /// Any per-balance fields we wish to add in the future.
    fields: bag::Bag,
}

/// === Accumulator object count storage ===

/// Key for storing the net count of accumulator objects as a dynamic field on the accumulator root.
public struct AccumulatorObjectCountKey() has copy, drop, store;

/// Records changes in the net count of accumulator objects. Called by the barrier transaction
/// as part of accumulator settlement.
///
/// This value is copied to the Sui system state object at end-of-epoch by the
/// WriteAccumulatorStorageCost transaction, for use in storage fund accounting. Copying once
/// at end-of-epoch lets us avoid depending on the Sui system state object in the settlement
/// barrier transaction.
#[allow(unused_function)]
fun record_accumulator_object_changes(
    accumulator_root: &mut AccumulatorRoot,
    objects_created: u64,
    objects_destroyed: u64,
) {
    let key = AccumulatorObjectCountKey();
    if (dynamic_field::exists_(accumulator_root.id_mut(), key)) {
        let current_count: &mut u64 = dynamic_field::borrow_mut(accumulator_root.id_mut(), key);
        assert!(*current_count + objects_created >= objects_destroyed, EInvariantViolation);
        *current_count = *current_count + objects_created - objects_destroyed;
    } else {
        assert!(objects_created >= objects_destroyed, EInvariantViolation);
        dynamic_field::add(accumulator_root.id_mut(), key, objects_created - objects_destroyed);
    };
}

/// Returns the current count of accumulator objects stored as a dynamic field.
#[allow(unused_function)]
fun get_accumulator_object_count(accumulator_root: &AccumulatorRoot): u64 {
    let key = AccumulatorObjectCountKey();
    if (dynamic_field::exists_(accumulator_root.id(), key)) {
        *dynamic_field::borrow(accumulator_root.id(), key)
    } else {
        0
    }
}
