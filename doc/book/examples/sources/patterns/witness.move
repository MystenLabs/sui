// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module that defines a generic type `Guardian<T>` which can only be
/// instantiated with a witness.
module examples::guardian {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    /// Phantom parameter T can only be initialized in the `create_guardian`
    /// function. But the types passed here must have `drop`.
    struct Guardian<phantom T: drop> has key, store {
        id: UID
    }

    /// The first argument of this function is an actual instance of the
    /// type T with `drop` ability. It is dropped as soon as received.
    public fun create_guardian<T: drop>(
        _witness: T, ctx: &mut TxContext
    ): Guardian<T> {
        Guardian { id: object::new(ctx) }
    }
}

/// Custom module that makes use of the `guardian`.
module examples::peace_guardian {
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    // Use the `guardian` as a dependency.
    use 0x0::guardian;

    /// This type is intended to be used only once.
    struct PEACE has drop {}

    #[allow(unused_function)]
    /// Module initializer is the best way to ensure that the
    /// code is called only once. With `Witness` pattern it is
    /// often the best practice.
    fun init(ctx: &mut TxContext) {
        transfer::public_transfer(
            guardian::create_guardian(PEACE {}, ctx),
            tx_context::sender(ctx)
        )
    }
}
