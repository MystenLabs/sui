// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator;

use std::type_name;
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

public struct Key has copy, drop, store {
    address: address,
    ty: type_name::TypeName,
}

public(package) fun get_accumulator_field_name<T>(address: address): Key {
    let ty = type_name::get_with_original_ids<T>();
    Key { address, ty }
}

public(package) fun get_accumulator_field_address<T>(address: address): address {
    let key = get_accumulator_field_name<T>(address);
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

#[allow(unused_function)]
fun settlement_prologue(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
}

public struct AccumulatorU128 has store {
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

    let name = get_accumulator_field_name<T>(owner);
    let root_id = &mut accumulator_root.id;

    if (dynamic_field::exists_with_type<Key, AccumulatorU128>(root_id, name)) {
        let value: &mut AccumulatorU128 = dynamic_field::borrow_mut(root_id, name);
        value.value = value.value + merge - split;
    } else {
        // cannot split if the field does not yet exist
        assert!(split == 0, EInvalidSplitAmount);
        let value = AccumulatorU128 {
            value: merge,
        };
        dynamic_field::add(root_id, name, value);
    };
}
