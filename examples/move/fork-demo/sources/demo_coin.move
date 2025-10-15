// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module fork_demo::demo_coin {
    use sui::coin::{Self, TreasuryCap};

    public struct DEMO_COIN has drop {}

    #[allow(deprecated_usage)]
    fun init(witness: DEMO_COIN, ctx: &mut TxContext) {
        let (treasury, metadata) = coin::create_currency(
            witness,
            6,
            b"DEMO",
            b"Demo Coin",
            b"A demo coin for testing fork functionality",
            option::none(),
            ctx
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, ctx.sender());
    }

    public fun mint(
        treasury: &mut TreasuryCap<DEMO_COIN>,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let coin = coin::mint(treasury, amount, ctx);
        transfer::public_transfer(coin, recipient);
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(DEMO_COIN {}, ctx);
    }
}
