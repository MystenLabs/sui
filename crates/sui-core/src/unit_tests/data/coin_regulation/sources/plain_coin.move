// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin_regulation::plain_coin;

use sui::coin;

public struct PLAIN_COIN has drop {}

fun init(otw: PLAIN_COIN, ctx: &mut TxContext) {
    let (mut treasury_cap, metadata) = coin::create_currency(
        otw,
        9,
        b"PC",
        b"PLAIN_COIN",
        b"An unregulated coin",
        option::none(),
        ctx,
    );
    transfer::public_transfer(treasury_cap.mint(1_000, ctx), ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}
