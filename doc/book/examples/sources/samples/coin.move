// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::my_coin {
    use sui::coin;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// The type identifier of coin. The coin will have a type
    /// tag of kind: `Coin<package_id::my_coin::MYCOIN>`
    struct MYCOIN has drop {}

    /// Module initializer is called once on module publish. A treasury
    /// cap is sent to the publisher, who then controls minting and burning
    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            coin::create_currency(MYCOIN {}, ctx),
            tx_context::sender(ctx)
        )
    }
}
