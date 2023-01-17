// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin_read::test_coin {

    use sui::tx_context::{Self, TxContext};
    use sui::coin;
    use sui::url;
    use std::option;
    use sui::transfer;

    struct TEST_COIN has drop {}

    fun init(witness: TEST_COIN, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency<TEST_COIN>(
            witness, 
            2,
            b"TEST_COIN",
            b"Test Coin",
            b"Test coin metadata",
            option::some(url::new_unsafe_from_bytes(b"http://sui.io")),
            ctx
        );

        coin::mint_and_transfer<TEST_COIN>(&mut treasury_cap, 5, tx_context::sender(ctx), ctx);
        coin::mint_and_transfer<TEST_COIN>(&mut treasury_cap, 6, tx_context::sender(ctx), ctx);

        transfer::share_object(metadata);
        transfer::share_object(treasury_cap)

    }
}