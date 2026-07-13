// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::move_random;

use std::vector;
use sui::object::{Self, UID};
use sui::transfer;
use sui::tx_context::TxContext;

public struct Object has key, store {
    id: UID,
    data: vector<u64>,
}

// simple infinite loop to go out of gas in computation
public fun loopy() {
    loop {}
}

// create an object with a vector of size `size` and transfer to recipient
public fun storage_heavy(mut size: u64, recipient: address, ctx: &mut TxContext) {
    let mut data = vector[];
    while (size > 0) {
        data.push_back(size);
        size = size - 1;
    };
    transfer::public_transfer(
        Object { id: object::new(ctx), data },
        recipient,
    )
}

// Function that always aborts to test gas price capping
public fun always_abort() {
    abort (42)
}
