// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module generic_event::generic_event {
    use sui::event;

    public struct GenericEvent<phantom T> has copy, drop {
        value: u64,
    }

    public fun emit_u64() {
        event::emit(GenericEvent<u64> { value: 1 });
    }

    public fun emit_address() {
        event::emit(GenericEvent<address> { value: 2 });
    }
}
