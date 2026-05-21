// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises regular and authenticated (light-client verifiable) event emission.
module move_building_blocks::events {
    use sui::event;

    public struct SimpleEvent has copy, drop {
        value: u64,
        who: address,
    }

    public struct DataEvent has copy, drop {
        data: vector<u8>,
    }

    public fun emit_simple(value: u64, who: address) {
        event::emit(SimpleEvent { value, who });
    }

    public fun emit_data(data: vector<u8>) {
        event::emit(DataEvent { data });
    }

    /// Authenticated event streams (`enable_authenticated_event_streams`).
    public fun emit_authenticated_simple(value: u64, who: address) {
        event::emit_authenticated(SimpleEvent { value, who });
    }
}
