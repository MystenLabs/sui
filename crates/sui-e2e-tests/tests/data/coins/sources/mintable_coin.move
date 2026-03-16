// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coins::mintable_coin;

use sui::coin::{Self, TreasuryCap};

public struct MINTABLE_COIN has drop {}

fun init(otw: MINTABLE_COIN, ctx: &mut TxContext) {
    let (treasury_cap, metadata) = coin::create_currency(
        otw,
        9,
        b"MINT",
        b"Mintable Coin",
        b"A coin with unfrozen TreasuryCap for testing",
        option::none(),
        ctx,
    );
    transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
    transfer::public_freeze_object(metadata);
}

public fun mint_and_transfer(
    treasury_cap: &mut TreasuryCap<MINTABLE_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let coin = treasury_cap.mint(amount, ctx);
    transfer::public_transfer(coin, recipient);
}
