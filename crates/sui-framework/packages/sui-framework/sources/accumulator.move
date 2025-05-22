// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator;

use std::type_name;
use sui::dynamic_field;
use sui::object::sui_accumulator_root_address;

public use fun accumulator_id as Accumulator.id;

const ENotSystemAddress: u64 = 0;

public struct Accumulator has key {
    id: UID,
}

public fun accumulator_id(acc: &mut Accumulator): &mut UID {
    &mut acc.id
}

#[allow(unused_function)]
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(Accumulator {
        id: object::sui_accumulator_root_object_id(),
    })
}

/// No-op called by settlement transactions to ensure that they are unique.
entry fun commit_to_checkpoint(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
}

public struct Key has copy, drop, store {
    address: address,
    ty: vector<u8>,
}

public(package) fun get_accumulator_field_name<T>(address: address): Key {
    let ty = type_name::get_with_original_ids<T>().into_string().into_bytes();
    Key { address, ty }
}

public(package) fun get_accumulator_field_address<T>(address: address): address {
    let key = get_accumulator_field_name<T>(address);
    dynamic_field::hash_type_and_key(sui_accumulator_root_address(), key)
}