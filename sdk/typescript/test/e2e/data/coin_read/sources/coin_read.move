// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin_read::test_coin {

    use sui::tx_context::{Self, TxContext};
    use sui::coin;
    use sui::transfer;

    struct TEST has drop {}

    fun init(witness: TEST, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency<TEST>(
            witness, 
            2,
            b"TEST",
            b"Test Coin",
            b"Test coin metadata",
            ctx
        );

        coin::mint_and_transfer<TEST>(&mut treasury_cap, 5, tx_context::sender(ctx), ctx);
        coin::mint_and_transfer<TEST>(&mut treasury_cap, 6, tx_context::sender(ctx), ctx);

    }
}