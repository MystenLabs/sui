// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin::a_coin;

use sui::coin::{Self, TreasuryCap};
use sui::balance;

// The type identifier of coin. The coin will have a type
// tag of kind: `Coin<package_object::a_coin::A_COIN>`
// Make sure that the name of the type matches the module's name.
public struct A_COIN has drop {}

// Module initializer is called once on module publish. A treasury
// cap is sent to the publisher, who then controls minting and burning.
#[allow(deprecated_usage)]
fun init(witness: A_COIN, ctx: &mut TxContext) {
    let (mut treasury, metadata) = coin::create_currency(
        witness,
        6,
        b"A_COIN",
        b"",
        b"",
        option::none(),
        ctx,
    );
    // Freezing this object makes the metadata immutable, including the title, name, and icon image.
    // If you want to allow mutability, share it with public_share_object instead.
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury, ctx.sender())
}

// Create A_COINS using the TreasuryCap.
public fun mint_coin(
    treasury_cap: &mut TreasuryCap<A_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let coin = coin::mint(treasury_cap, amount, ctx);
    transfer::public_transfer(coin, recipient)
}

// Mint to address balance
public fun mint_balance(
    treasury_cap: &mut TreasuryCap<A_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    balance::send_funds(coin::into_balance(coin::mint(treasury_cap, amount, ctx)), recipient);
}
