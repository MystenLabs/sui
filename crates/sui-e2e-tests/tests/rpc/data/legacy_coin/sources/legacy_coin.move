// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example coin using the legacy create_currency API (v1)
#[allow(deprecated_usage)]
module legacy_coin::legacy_coin;

use sui::coin::{Self, TreasuryCap};
use sui::url;

/// Name of the coin
public struct LEGACY_COIN has drop {}

/// Register the legacy currency using create_currency v1 API
fun init(otw: LEGACY_COIN, ctx: &mut TxContext) {
    let (treasury_cap, metadata) = coin::create_currency<LEGACY_COIN>(
        otw,
        8, // decimals
        b"LEGACY", // symbol
        b"Legacy Coin", // name
        b"Legacy coin for testing GetCoinInfo fallback", // description
        option::some(url::new_unsafe_from_bytes(b"https://example.com/legacy.png")), // icon_url
        ctx,
    );

    // Freeze metadata so it becomes immutable
    transfer::public_freeze_object(metadata);

    // Transfer treasury cap to sender
    transfer::public_transfer(treasury_cap, ctx.sender())
}

/// Mint new coins
public fun mint(
    treasury_cap: &mut TreasuryCap<LEGACY_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let coin = treasury_cap.mint(amount, ctx);
    transfer::public_transfer(coin, recipient);
}
