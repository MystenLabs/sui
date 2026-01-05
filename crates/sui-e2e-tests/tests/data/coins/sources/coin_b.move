// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(deprecated_usage)]
module coins::coin_b;

use sui::coin;

public struct COIN_B has drop {}

fun init(otw: COIN_B, ctx: &mut TxContext) {
    let (mut treasury_cap, metadata) = coin::create_currency(
        otw,
        9,
        b"COIN_B",
        b"Coin B",
        b"Test coin B",
        option::none(),
        ctx,
    );
    let coin = treasury_cap.mint(10000, ctx);
    coin.send_funds(tx_context::sender(ctx));
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}
