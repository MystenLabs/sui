// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module emit_test_event::emit_test_event {
    use sui::event;

    public struct TestEvent has copy, drop {
        value: u64,
    }

    public fun emit_test_event() {
        event::emit(TestEvent {
            value: 1,
        });
    }

    public fun emit_with_value(value: u64) {
        event::emit(TestEvent { value })
    }
}
