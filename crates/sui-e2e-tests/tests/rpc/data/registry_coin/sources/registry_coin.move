// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example coin using the new CoinRegistry API
module registry_coin::registry_coin;

use sui::coin::{Self, TreasuryCap};
use sui::coin_registry::{Self, MetadataCap, CoinRegistry, Currency};

/// Name of the coin
public struct REGISTRY_COIN has drop {}

/// Register the currency using new_currency_with_otw API
fun init(otw: REGISTRY_COIN, ctx: &mut TxContext) {
    // Create the currency with OTW
    let (currency_init, treasury_cap) = coin_registry::new_currency_with_otw<REGISTRY_COIN>(
        otw,
        6, // decimals
        b"REGISTRY".to_string(),
        b"Registry Coin".to_string(),
        b"Registry coin for testing GetCoinInfo with CoinRegistry".to_string(),
        b"https://example.com/registry.png".to_string(),
        ctx,
    );

    // Finalize - this will transfer the Currency to the registry (0xc)
    let metadata_cap = currency_init.finalize(ctx);

    // Note: Someone needs to call finalize_registration after this to complete
    // the registration at the derived address

    // Transfer caps to sender
    transfer::public_transfer(treasury_cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
}

/// Mint new coins
public fun mint(
    treasury_cap: &mut TreasuryCap<REGISTRY_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let coin = treasury_cap.mint(amount, ctx);
    transfer::public_transfer(coin, recipient);
}

/// Update coin metadata using MetadataCap
public fun update_name(
    currency: &mut Currency<REGISTRY_COIN>,
    metadata_cap: &MetadataCap<REGISTRY_COIN>,
    new_name: vector<u8>,
) {
    currency.set_name(metadata_cap, new_name.to_string());
}

/// Register the supply after minting, consuming the TreasuryCap
public fun register_supply(
    currency: &mut Currency<REGISTRY_COIN>,
    treasury_cap: TreasuryCap<REGISTRY_COIN>,
) {
    currency.make_supply_fixed(treasury_cap);
}
