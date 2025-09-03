// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements a coin with a deflationary supply. Upon initialization, mint the
/// total supply and give up `TreasuryCap` to make the supply deflationary (prevents
/// minting but allows burning).
///
/// Keep the ability to update Currency metadata, such as name, symbol,
/// description, and icon URL.
module examples::deflationary_supply;

use sui::coin::Coin;
use sui::coin_registry::{Self, Currency};

// Total supply of the `DEFLATIONARY_SUPPLY` coin is 1B (with 6 decimals).
const TOTAL_SUPPLY: u64 = 1000000000_000000;

// The type identifier of coin. The coin will have a type
// tag of kind: `Coin<package_object::deflationary_supply::DEFLATIONARY_SUPPLY>`
public struct DEFLATIONARY_SUPPLY has drop {}

// Module initializer is called once on module publish.
// - `TreasuryCap` is given up to the `Currency`
// - Total supply is sent to the publisher along with `MetadataCap`
fun init(witness: DEFLATIONARY_SUPPLY, ctx: &mut TxContext) {
    let (mut builder, mut treasury_cap) = coin_registry::new_currency(
        witness,
        6, // Decimals
        b"DEFLATIONARY_SUPPLY".to_string(), // Symbol
        b"Deflationary Supply Coin".to_string(), // Name
        b"Cannot be minted, but can be burned".to_string(), // Description
        b"https://example.com/my_coin.png".to_string(), // Icon URL
        ctx,
    );

    // Use the `TreasuryCap` to mint the total supply of the coin.
    let total_supply = treasury_cap.mint(TOTAL_SUPPLY, ctx);

    // Make the supply deflationary by giving up TreasuryCap.
    builder.inner_mut().make_supply_deflationary(treasury_cap);

    // Finalize the building process and claim the metadata cap.
    let metadata_cap = builder.finalize(ctx);

    // Transfer the minted supply and metadata cap to the publisher.
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_transfer(total_supply, ctx.sender());
}

/// Method is for demonstration purposes only.
/// This call can be performed directly on the `Currency` object.
public fun burn(currency: &mut Currency<DEFLATIONARY_SUPPLY>, coin: Coin<DEFLATIONARY_SUPPLY>) {
    currency.burn(coin);
}
