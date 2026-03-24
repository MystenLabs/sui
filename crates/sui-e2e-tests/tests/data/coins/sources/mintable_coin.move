// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coins::mintable_coin;

use sui::coin::{Self, TreasuryCap};

public struct MINTABLE_COIN has drop {}

fun init(witness: MINTABLE_COIN, ctx: &mut TxContext) {
    let (treasury_cap, metadata) = coin::create_currency(
        witness,
        9,
        b"MINT",
        b"Mintable Coin",
        b"A test coin with an unfrozen TreasuryCap",
        option::none(),
        ctx,
    );
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury_cap, ctx.sender());
}

public fun mint_and_transfer(
    treasury_cap: &mut TreasuryCap<MINTABLE_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let coin = coin::mint(treasury_cap, amount, ctx);
    transfer::public_transfer(coin, recipient);
}
