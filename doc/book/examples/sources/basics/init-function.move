// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::one_timer {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};

    /// The one of a kind - created in the module initializer.
    struct CreatorCapability has key {
        id: UID
    }

    #[allow(unused_function)]
    /// This function is only called once on module publish.
    /// Use it to make sure something has happened only once, like
    /// here - only module author will own a version of a
    /// `CreatorCapability` struct.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(CreatorCapability {
            id: object::new(ctx),
        }, tx_context::sender(ctx))
    }
}
