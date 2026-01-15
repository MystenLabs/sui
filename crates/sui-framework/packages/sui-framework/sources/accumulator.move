// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator;

use sui::dynamic_field;
use sui::object::sui_accumulator_root_address;

const ENotSystemAddress: u64 = 0;

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

public(package) fun root_id(accumulator_root: &AccumulatorRoot): &UID {
    &accumulator_root.id
}

public use fun root_id as AccumulatorRoot.id;

public(package) fun root_id_mut(accumulator_root: &mut AccumulatorRoot): &mut UID {
    &mut accumulator_root.id
}

public use fun root_id_mut as AccumulatorRoot.id_mut;

// TODO: these u128-specific functions will need to be generalized (somehow) if we add support
// for other types.
public(package) fun accumulator_u128_exists<T>(root: &AccumulatorRoot, address: address): bool {
    root.has_accumulator<T, U128>(Key<T> { address })
}

public use fun accumulator_u128_exists as AccumulatorRoot.u128_exists;

public(package) fun accumulator_u128_read<T>(root: &AccumulatorRoot, address: address): u128 {
    let accumulator = root.borrow_accumulator<T, U128>(Key<T> { address });
    accumulator.value
}

public use fun accumulator_u128_read as AccumulatorRoot.u128_read;

// === Accumulator value types ===

/// Storage for 128-bit accumulator values.
///
/// Currently only used to represent the sum of 64 bit values (such as `Balance<T>`).
/// The additional bits are necessary to prevent overflow, as it would take 2^64 deposits of U64_MAX
/// to cause an overflow.
public struct U128 has store {
    value: u128,
}

public(package) fun create_u128(value: u128): U128 {
    U128 { value }
}

public(package) fun destroy_u128(u128: U128) {
    let U128 { value: _ } = u128;
}

public use fun destroy_u128 as U128.destroy;

public(package) fun update_u128(u128: &mut U128, merge: u128, split: u128) {
    u128.value = u128.value + merge - split;
}

public use fun update_u128 as U128.update;

public(package) fun is_zero_u128(u128: &U128): bool {
    u128.value == 0
}

public use fun is_zero_u128 as U128.is_zero;

// === Accumulator address computation ===

/// `Key` is used only for computing the field id of accumulator objects.
/// `T` is the type of the accumulated value, e.g. `Balance<SUI>`
public struct Key<phantom T> has copy, drop, store {
    address: address,
}

public(package) fun accumulator_key<T>(address: address): Key<T> {
    Key { address }
}

public(package) fun accumulator_address<T>(address: address): address {
    let key = Key<T> { address };
    dynamic_field::hash_type_and_key(sui_accumulator_root_address(), key)
}

// === Adding, removing, and mutating accumulator objects ===

/// Balance object methods
public(package) fun root_has_accumulator<K, V: store>(
    accumulator_root: &AccumulatorRoot,
    name: Key<K>,
): bool {
    dynamic_field::exists_with_type<Key<K>, V>(&accumulator_root.id, name)
}

public use fun root_has_accumulator as AccumulatorRoot.has_accumulator;

public(package) fun root_add_accumulator<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: Key<K>,
    value: V,
) {
    dynamic_field::add(&mut accumulator_root.id, name, value);
}

public use fun root_add_accumulator as AccumulatorRoot.add_accumulator;

public(package) fun root_borrow_accumulator_mut<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: Key<K>,
): &mut V {
    dynamic_field::borrow_mut<Key<K>, V>(&mut accumulator_root.id, name)
}

public use fun root_borrow_accumulator_mut as AccumulatorRoot.borrow_accumulator_mut;

public(package) fun root_borrow_accumulator<K, V: store>(
    accumulator_root: &AccumulatorRoot,
    name: Key<K>,
): &V {
    dynamic_field::borrow<Key<K>, V>(&accumulator_root.id, name)
}

public use fun root_borrow_accumulator as AccumulatorRoot.borrow_accumulator;

public(package) fun root_remove_accumulator<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: Key<K>,
): V {
    dynamic_field::remove<Key<K>, V>(&mut accumulator_root.id, name)
}

public use fun root_remove_accumulator as AccumulatorRoot.remove_accumulator;

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
