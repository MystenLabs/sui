// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A shared counter for exercising publish, call, and upgrade on a fork.
module counter::counter;

public struct Counter has key {
    id: UID,
    value: u64,
}

/// Create a shared counter starting at zero.
public fun create(ctx: &mut TxContext) {
    transfer::share_object(Counter { id: object::new(ctx), value: 0 })
}

/// Increment the counter by one.
public fun increment(counter: &mut Counter) {
    counter.value = counter.value + 1;
}

public fun value(counter: &Counter): u64 {
    counter.value
}
