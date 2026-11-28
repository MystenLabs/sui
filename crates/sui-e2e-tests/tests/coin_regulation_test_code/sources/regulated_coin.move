// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin_regulation_test_code::regulated_coin;

use sui::coin;

public struct REGULATED_COIN has drop {}

const NUM_COINS: u64 = 8;

fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
        otw,
        9,
        b"RC",
        b"REGULATED_COIN",
        b"A regulated coin",
        option::none(),
        true,
        ctx,
    );
    NUM_COINS.do!(|_| transfer::public_transfer(treasury_cap.mint(1_000, ctx), ctx.sender()));
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}
