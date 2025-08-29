// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module emit_event::emit_event {
    use sui::event;

    public struct TestEvent has copy, drop {
        message: vector<u8>,
        value: u64,
    }

    // This init function will automatically run when the package is published
    // and will emit an event that we can test for
    fun init(_ctx: &mut TxContext) {
        event::emit(TestEvent {
            message: b"Package published successfully!",
            value: 42,
        });
    }
}
