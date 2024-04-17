// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::publish_with_event {
    use std::ascii::{Self, String};

    use sui::event;
    use sui::tx_context::TxContext;

    public struct PublishEvent has copy, drop {
        foo: String
    }

    fun init(_ctx: &mut TxContext) {
        event::emit(PublishEvent { foo: ascii::string(b"bar") })
    }
}
