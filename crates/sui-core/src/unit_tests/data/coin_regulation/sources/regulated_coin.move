// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin_regulation::regulated_coin;

use sui::coin::{Self, Coin};

public struct REGULATED_COIN has drop {}

fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
        otw,
        9,
        b"RC",
        b"REGULATED_COIN",
        b"A new regulated coin",
        option::none(),
        true,
        ctx,
    );
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_transfer(treasury_cap.mint(1_000, ctx), ctx.sender());
    transfer::public_transfer(treasury_cap.mint(1_000, ctx), ctx.sender());
    treasury_cap.mint(1_000, ctx).send_funds(ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}
