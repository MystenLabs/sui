// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example regulated coin using create_regulated_currency_v3 API
module regulated_coin::regulated_coin;

use sui::coin::{Self, TreasuryCap, DenyCapV2};
use sui::coin_registry::{Self, MetadataCap, CoinRegistry, Currency};
use sui::deny_list;

/// Name of the coin
public struct REGULATED_COIN has drop {}

/// Register the regulated currency using new CoinRegistry API
fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    // Create the currency
    let (mut currency_init, treasury_cap) = coin_registry::new_currency_with_otw<REGULATED_COIN>(
        otw,
        9, // decimals
        b"REG".to_string(),
        b"Regulated Coin".to_string(),
        b"Regulated coin for testing GetCoinInfo with CoinRegistry".to_string(),
        b"https://example.com/regulated.png".to_string(),
        ctx,
    );

    // Make it regulated with a deny cap
    let deny_cap = currency_init.make_regulated(true, ctx);

    // Finalize the currency registration and get the metadata cap
    let metadata_cap = currency_init.finalize(ctx);

    // Transfer caps to sender
    transfer::public_transfer(treasury_cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
}

/// Mint new coins
public fun mint(
    treasury_cap: &mut TreasuryCap<REGULATED_COIN>,
    amount: u64,
    recipient: address,
    ctx: &mut TxContext,
) {
    let coin = treasury_cap.mint(amount, ctx);
    transfer::public_transfer(coin, recipient);
}

/// Add an address to the deny list
public fun deny_address(
    deny_list: &mut deny_list::DenyList,
    deny_cap: &mut DenyCapV2<REGULATED_COIN>,
    address_to_deny: address,
    ctx: &mut TxContext,
) {
    coin::deny_list_v2_add(deny_list, deny_cap, address_to_deny, ctx);
}
