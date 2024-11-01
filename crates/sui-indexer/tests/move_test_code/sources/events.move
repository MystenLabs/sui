// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


module move_test_code::events_queries {
    use sui::event;

    public struct EventA has copy, drop {
        new_value: u64
    }

    public entry fun emit_1(value: u64) {
        event::emit(EventA { new_value: value })
    }

    public entry fun emit_2(value: u64) {
        event::emit(EventA { new_value: value });
        event::emit(EventA { new_value: value + 1})
    }

        public entry fun emit_3(value: u64) {
        event::emit(EventA { new_value: value });
        event::emit(EventA { new_value: value + 1});
        event::emit(EventA { new_value: value + 2});
    }
}
