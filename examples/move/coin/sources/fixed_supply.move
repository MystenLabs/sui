// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements a coin with a fixed supply. Upon initialization, mint the total
/// supply and give up `TreasuryCap` to freeze the supply (prevents minting and
/// burning).
///
/// Keep the ability to update Currency metadata, such as name, symbol,
/// description, and icon URL.
module examples::fixed_supply;

use sui::coin_registry;

// Total supply of the `FIXED_SUPPLY` coin is 1B (with 6 decimals).
const TOTAL_SUPPLY: u64 = 1000000000_000000;

// The type identifier of coin. The coin will have a type
// tag of kind: `Coin<package_object::fixed_supply::FIXED_SUPPLY>`
public struct FIXED_SUPPLY has drop {}

// Module initializer is called once on module publish.
// - `TreasuryCap` is locked up in the `Currency`
// - Total supply is sent to the publisher along with `MetadataCap`
fun init(witness: FIXED_SUPPLY, ctx: &mut TxContext) {
    let (mut currency, mut treasury_cap) = coin_registry::new_currency_with_otw(
        witness,
        6, // Decimals
        b"FIXED_SUPPLY".to_string(), // Symbol
        b"Fixed Supply Coin".to_string(), // Name
        b"Cannot be minted nor burned".to_string(), // Description
        b"https://example.com/my_coin.png".to_string(), // Icon URL
        ctx,
    );

    // Use the `TreasuryCap` to mint the total supply of the coin.
    let total_supply = treasury_cap.mint(TOTAL_SUPPLY, ctx);

    // Make the supply fixed by giving up TreasuryCap.
    currency.make_supply_fixed(treasury_cap);

    // Finalize the building process and claim the metadata cap.
    let metadata_cap = currency.finalize(ctx);

    // Transfer the minted supply and metadata cap to the publisher.
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_transfer(total_supply, ctx.sender());
}
