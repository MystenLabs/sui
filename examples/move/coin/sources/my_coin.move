// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::my_coin;

use sui::coin::{Self, TreasuryCap};

// The type identifier of coin. The coin will have a type
// tag of kind: `Coin<package_object::mycoin::MYCOIN>`
// Make sure that the name of the type matches the module's name.
public struct MY_COIN has drop {}

// Module initializer is called once on module publish. A treasury
// cap is sent to the publisher, who then controls minting and burning.
fun init(witness: MY_COIN, ctx: &mut TxContext) {
    let (treasury, metadata) = coin::create_currency(
        witness,
        6,
        b"MY_COIN",
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

// Create MY_COINs using the TreasuryCap.
public fun mint(
    treasury_cap: &mut TreasuryCap<MY_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let coin = coin::mint(treasury_cap, amount, ctx);
    transfer::public_transfer(coin, recipient)
}
