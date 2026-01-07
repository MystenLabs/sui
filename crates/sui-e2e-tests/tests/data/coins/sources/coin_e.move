// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(deprecated_usage)]
module coins::coin_e;

use sui::coin;

public struct COIN_E has drop {}

fun init(otw: COIN_E, ctx: &mut TxContext) {
    let (mut treasury_cap, metadata) = coin::create_currency(
        otw,
        9,
        b"COIN_E",
        b"Coin E",
        b"Test coin E",
        option::none(),
        ctx,
    );
    let coin = treasury_cap.mint(10000, ctx);
    coin.send_funds(tx_context::sender(ctx));
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}
