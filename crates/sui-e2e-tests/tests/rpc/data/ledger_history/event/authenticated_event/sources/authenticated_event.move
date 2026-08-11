// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module authenticated_event::authenticated_event {
    use sui::event;

    public struct NormalEvent has copy, drop {
        value: u64,
    }

    public struct AuthenticatedEvent has copy, drop {
        value: u64,
    }

    public fun emit_both() {
        event::emit(NormalEvent { value: 1 });
        event::emit_authenticated(AuthenticatedEvent { value: 2 });
    }
}
