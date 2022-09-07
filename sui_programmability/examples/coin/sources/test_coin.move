// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A flash loan that works for any Coin type
module test_coin::mycoin {
    use sui::coin;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// The type identifier of coin. The coin will have a type
    /// tag of kind: `Coin<package_object::my_coin::MYCOIN>`
    struct MYCOIN has drop {}

    /// Module initializer is called once on module publish. A treasury
    /// cap is sent to the publisher, who then controls minting and burning
    fun init(coin: MYCOIN, ctx: &mut TxContext) {
        transfer::transfer(
            coin::create_currency(coin, ctx),
            tx_context::sender(ctx)
        )
    }
}
