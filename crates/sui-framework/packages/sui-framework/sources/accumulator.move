// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator;

use sui::dynamic_field;
use sui::object::sui_accumulator_root_address;

const ENotSystemAddress: u64 = 0;
const EInvalidSplitAmount: u64 = 1;

public struct AccumulatorRoot has key {
    id: UID,
}

#[allow(unused_function)]
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(AccumulatorRoot {
        id: object::sui_accumulator_root_object_id(),
    })
}

// === Accumulator address computation ===

/// `Key` is used only for computing the field id of accumulator objects.
/// `T` is the type of the accumulated value, e.g. `Balance<SUI>`
public struct Key<phantom T> has copy, drop, store {
    address: address,
}

public(package) fun accumulator_address<T>(address: address): address {
    let key = Key<T> { address };
    dynamic_field::hash_type_and_key(sui_accumulator_root_address(), key)
}

// === Adding, removing, and mutating accumulator objects ===

/// Balance object methods
fun root_has_accumulator<K, V: store>(accumulator_root: &AccumulatorRoot, name: Key<K>): bool {
    dynamic_field::exists_with_type<Key<K>, V>(&accumulator_root.id, name)
}

use fun root_has_accumulator as AccumulatorRoot.has_accumulator;

fun root_add_accumulator<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: Key<K>,
    value: V,
) {
    dynamic_field::add(&mut accumulator_root.id, name, value);
}

use fun root_add_accumulator as AccumulatorRoot.add_accumulator;

fun root_borrow_accumulator_mut<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: Key<K>,
): &mut V {
    dynamic_field::borrow_mut<Key<K>, V>(&mut accumulator_root.id, name)
}

use fun root_borrow_accumulator_mut as AccumulatorRoot.borrow_accumulator_mut;

fun root_remove_accumulator<K, V: store>(accumulator_root: &mut AccumulatorRoot, name: Key<K>): V {
    dynamic_field::remove<Key<K>, V>(&mut accumulator_root.id, name)
}

use fun root_remove_accumulator as AccumulatorRoot.remove_accumulator;

// === Settlement storage types and entry points ===

/// Storage for 128-bit accumulator values.
///
/// Currently only used to represent the sum of 64 bit values (such as `Balance<T>`).
/// The additional bits are necessary to prevent overflow, as it would take 2^64 deposits of U64_MAX
/// to cause an overflow.
public struct U128 has store {
    value: u128,
}

/// Called by settlement transactions to ensure that the settlement transaction has a unique
/// digest.
#[allow(unused_function)]
fun settlement_prologue(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
}

#[allow(unused_function)]
fun settle_u128<T>(
    accumulator_root: &mut AccumulatorRoot,
    owner: address,
    merge: u128,
    split: u128,
    ctx: &TxContext,
) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    // Merge and split should be netted out prior to calling this function.
    assert!((merge == 0 ) != (split == 0), EInvalidSplitAmount);

    let name = Key<T> { address: owner };

    if (accumulator_root.has_accumulator<T, U128>(name)) {
        let is_zero = {
            let value: &mut U128 = accumulator_root.borrow_accumulator_mut(name);
            value.value = value.value + merge - split;

            value.value == 0
        };

        if (is_zero) {
            let U128 { value: _ } = accumulator_root.remove_accumulator<T, U128>(
                name,
            );
        }
    } else {
        // cannot split if the field does not yet exist
        assert!(split == 0, EInvalidSplitAmount);
        let value = U128 {
            value: merge,
        };

        accumulator_root.add_accumulator(name, value);
    };
}

// === Natives for emitting accumulator events ===

public(package) native fun emit_deposit_event<T>(
    accumulator: address,
    recipient: address,
    amount: u64,
);
public(package) native fun emit_withdraw_event<T>(
    accumulator: address,
    owner: address,
    amount: u64,
);
