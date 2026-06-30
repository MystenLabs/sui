// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin::c_coin;

use sui::coin::{Self, TreasuryCap};
use sui::balance;


public struct C_COIN has drop {}

#[allow(deprecated_usage)]
fun init(witness: C_COIN, ctx: &mut TxContext) {
    let (mut treasury, metadata) = coin::create_currency(
        witness,
        6,
        b"C_COIN",
        b"",
        b"",
        option::none(),
        ctx,
    );
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury, ctx.sender())
}

public fun mint_coin(
    treasury_cap: &mut TreasuryCap<C_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let coin = coin::mint(treasury_cap, amount, ctx);
    transfer::public_transfer(coin, recipient)
}

// Mint to address balance
public fun mint_balance(
    treasury_cap: &mut TreasuryCap<C_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    balance::send_funds(coin::into_balance(coin::mint(treasury_cap, amount, ctx)), recipient);
}
