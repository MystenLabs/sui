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

/// Balance object methods

/// The key type to look up a balance object.
public struct AccumulatorName<phantom T> has copy, drop, store {
    address: address,
}

fun accumulator_root_has_balance<K, V: store>(
    accumulator_root: &AccumulatorRoot,
    name: AccumulatorName<K>,
): bool {
    dynamic_field::exists_with_type<AccumulatorName<K>, V>(&accumulator_root.id, name)
}

use fun accumulator_root_has_balance as AccumulatorRoot.has_balance;

fun accumulator_root_add_balance<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: AccumulatorName<K>,
    value: V,
) {
    dynamic_field::add(&mut accumulator_root.id, name, value);
}

use fun accumulator_root_add_balance as AccumulatorRoot.add_balance;

fun accumulator_root_borrow_balance_mut<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: AccumulatorName<K>,
): &mut V {
    dynamic_field::borrow_mut<AccumulatorName<K>, V>(&mut accumulator_root.id, name)
}

use fun accumulator_root_borrow_balance_mut as AccumulatorRoot.borrow_balance_mut;

fun accumulator_root_remove_balance<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: AccumulatorName<K>,
): V {
    dynamic_field::remove<AccumulatorName<K>, V>(&mut accumulator_root.id, name)
}

use fun accumulator_root_remove_balance as AccumulatorRoot.remove_balance;

public(package) fun get_accumulator_field_address<T>(address: address): address {
    let key = AccumulatorName<T> { address };
    dynamic_field::hash_type_and_key(sui_accumulator_root_address(), key)
}

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

/// Called by settlement transactions to ensure that the settlement transaction has a unique
/// digest.
#[allow(unused_function)]
fun settlement_prologue(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
}

/// A value type for storing any type that is represented in move as a u64.
/// The additional bits are to prevent overflow, as it would take 2^64 deposits of U64_MAX
/// to cause an overflow.
public struct U128 has store {
    value: u128,
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

    let name = AccumulatorName<T> { address: owner };

    if (accumulator_root.has_balance<T, U128>(name)) {
        let is_zero = {
            let value: &mut U128 = accumulator_root.borrow_balance_mut(name);
            value.value = value.value + merge - split;

            value.value == 0
        };

        if (is_zero) {
            let U128 { value: _ } = accumulator_root.remove_balance<T, U128>(
                name,
            );
        }
    } else {
        // cannot split if the field does not yet exist
        assert!(split == 0, EInvalidSplitAmount);
        let value = U128 {
            value: merge,
        };

        accumulator_root.add_balance(name, value);
    };
}
