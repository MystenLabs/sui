// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module execution_time_test::compute;

use sui::object::{Self, UID};
use sui::transfer;
use sui::tx_context::TxContext;

public struct Counter has key, store {
    id: UID,
    value: u64,
}

public entry fun create_counter(ctx: &mut TxContext) {
    let counter = Counter {
        id: object::new(ctx),
        value: 0,
    };
    transfer::public_share_object(counter);
}

public entry fun increment_a(counter: &mut Counter) {
    counter.value = counter.value + 1;
}

public entry fun increment_b(counter: &mut Counter) {
    counter.value = counter.value + 1;
}

public entry fun increment_c(counter: &mut Counter) {
    counter.value = counter.value + 1;
}
