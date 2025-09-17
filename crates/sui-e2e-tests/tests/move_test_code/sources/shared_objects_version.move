// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code::shared_objects_version;

public struct Counter has key {
    id: UID,
    value: u64,
}

public fun create_counter(ctx: &mut TxContext) {
    transfer::transfer(
        Counter {
            id: object::new(ctx),
            value: 0,
        },
        ctx.sender(),
    )
}

public fun create_shared_counter(ctx: &mut TxContext) {
    transfer::share_object(Counter {
        id: object::new(ctx),
        value: 0,
    })
}

public fun share_counter(counter: Counter) {
    transfer::share_object(counter)
}

public fun increment_counter(counter: &mut Counter) {
    counter.value = counter.value + 1
}
