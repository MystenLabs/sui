// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example coin using new_currency (without OTW)
module non_otw_coin::non_otw_coin;

use sui::{
	coin::{Self, TreasuryCap, Coin},
	coin_registry::{Self, MetadataCap, CoinRegistry, Currency},
	transfer::{Self, Receiving},
	tx_context::{Self, TxContext}
};

/// Type identifier for the coin (not a one-time witness)
/// This struct has a UID field, making it incompatible with OTW pattern
public struct MyCoin has key { id: UID }

/// Create a new currency without requiring a one-time witness
/// This demonstrates using new_currency API that doesn't require OTW
#[allow(lint(self_transfer))]
public fun create_currency(registry: &mut CoinRegistry, ctx: &mut TxContext) {
	// Create the currency without OTW
	let (currency_init, treasury_cap) = coin_registry::new_currency<MyCoin>(
		registry,
		7, // decimals
		b"NONOTW".to_string(),
		b"Non-OTW Coin".to_string(),
		b"Non-OTW coin for testing GetCoinInfo with new_currency (without OTW)".to_string(),
		b"https://example.com/non_otw.png".to_string(),
		ctx,
	);

	// Finalize - this will transfer the Currency to the registry (0xc)
	let metadata_cap = coin_registry::finalize(currency_init, ctx);

	// Transfer caps to sender
	transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
	transfer::public_transfer(metadata_cap, tx_context::sender(ctx));
}

/// Mint new coins
public fun mint(
	treasury_cap: &mut TreasuryCap<MyCoin>,
	amount: u64,
	recipient: address,
	ctx: &mut TxContext,
) {
	let coin = coin::mint<MyCoin>(treasury_cap, amount, ctx);
	transfer::public_transfer(coin, recipient);
}

/// Update coin metadata using MetadataCap
public fun update_symbol(
	currency: &mut Currency<MyCoin>,
	metadata_cap: &MetadataCap<MyCoin>,
	new_symbol: vector<u8>,
) {
	coin_registry::set_symbol(currency, metadata_cap, new_symbol.to_string());
}