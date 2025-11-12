// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module regulated_coin_example::regulated_coin;

use sui::coin_registry;

/// OTW for the coin.
public struct REGULATED_COIN has drop {}

fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    // Creates a new currency using `create_currency`, but with an extra capability that
    // allows for specific addresses to have their coins frozen. Those addresses cannot interact
    // with the coin as input objects.
    let (mut currency, treasury_cap) = coin_registry::new_currency_with_otw(
        otw,
        5, // Decimals
        b"$TABLE".to_string(), // Symbol
        b"RegulaCoin".to_string(), // Name
        b"Example Regulated Coin".to_string(), // Description
        b"https://example.com/regulated_coin.png".to_string(), // Icon URL
        ctx,
    );

    // Mark the currency as regulated, issue a `DenyCapV2`.
    let deny_cap = currency.make_regulated(true, ctx);
    let metadata_cap = currency.finalize(ctx);
    let sender = ctx.sender();

    // Transfer the treasury cap, deny cap, and metadata cap to the publisher.
    transfer::public_transfer(treasury_cap, sender);
    transfer::public_transfer(deny_cap, sender);
    transfer::public_transfer(metadata_cap, sender);
}
