// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A coin type where the metadata is transferred to the sender
// (this is not normally a recommended pattern)

module examples::owned_metadata_coin;

use sui::coin::{Self, TreasuryCap};

// The type identifier of coin. The coin will have a type
// tag of kind: `Coin<package_object::mycoin::MYCOIN>`
// Make sure that the name of the type matches the module's name.
public struct OWNED_METADATA_COIN has drop {}

#[allow(deprecated_usage)]
// Module initializer is called once on module publish. A treasury
// cap is sent to the publisher, who then controls minting and burning.
fun init(witness: OWNED_METADATA_COIN, ctx: &mut TxContext) {
    let (treasury, metadata) = coin::create_currency(
        witness,
        6,
        b"OWNED_METADATA_COIN",
        b"",
        b"",
        option::none(),
        ctx,
    );
    // Transfer metadata to the sender
    transfer::public_transfer(metadata, ctx.sender());
    transfer::public_transfer(treasury, ctx.sender())
}

// Create MY_COINs using the TreasuryCap.
public fun mint(
    treasury_cap: &mut TreasuryCap<OWNED_METADATA_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let coin = coin::mint(treasury_cap, amount, ctx);
    transfer::public_transfer(coin, recipient)
}
