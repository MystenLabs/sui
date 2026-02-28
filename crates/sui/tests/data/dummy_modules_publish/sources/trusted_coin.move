// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example coin with a trusted owner responsible for minting/burning (e.g., a stablecoin)
module examples::trusted_coin;

use sui::coin::{Self, Coin, TreasuryCap};

/// Name of the coin
public struct TRUSTED_COIN has drop {}

#[allow(deprecated_usage)]
/// Register the trusted currency to acquire its `TreasuryCap`. Because
/// this is a module initializer, it ensures the currency only gets
/// registered once.
fun init(witness: TRUSTED_COIN, ctx: &mut TxContext) {
    // Get a treasury cap for the coin and give it to the transaction
    // sender
    let (treasury_cap, metadata) = coin::create_currency(
        witness,
        2,
        b"TRUSTED",
        b"",
        b"",
        option::none(),
        ctx,
    );
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury_cap, ctx.sender())
}

public fun mint(
    treasury_cap: &mut TreasuryCap<TRUSTED_COIN>,
    amount: u64,
    ctx: &mut TxContext,
): Coin<TRUSTED_COIN> {
    treasury_cap.mint(amount, ctx)
}

public fun transfer(treasury_cap: TreasuryCap<TRUSTED_COIN>, recipient: address) {
    transfer::public_transfer(treasury_cap, recipient);
}
