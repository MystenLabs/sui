// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example coin using the BurnOnly supply state in CoinRegistry
module burnonly_coin::burnonly_coin;

use sui::{
	coin::{Self, TreasuryCap, Coin},
	coin_registry::{Self, MetadataCap, CoinRegistry, Currency},
	transfer::{Self, Receiving},
	tx_context::{Self, TxContext}
};

	/// Name of the coin
	public struct BURNONLY_COIN has drop {}

	/// Register the currency using new_currency_with_otw API
	fun init(witness: BURNONLY_COIN, ctx: &mut TxContext) {
		// Create the currency with OTW
		let (currency_init, treasury_cap) = coin_registry::new_currency_with_otw<BURNONLY_COIN>(
			witness,
			9, // decimals
			b"BURNONLY".to_string(),
			b"BurnOnly Coin".to_string(),
			b"BurnOnly coin for testing GetCoinInfo with CoinRegistry BurnOnly supply state".to_string(),
			b"https://example.com/burnonly.png".to_string(),
			ctx,
		);

		// Finalize - this will transfer the Currency to the registry (0xc)
		let metadata_cap = coin_registry::finalize(currency_init, ctx);

		// Note: Someone needs to call finalize_registration after this to complete
		// the registration at the derived address

		// Transfer caps to sender
		transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
		transfer::public_transfer(metadata_cap, tx_context::sender(ctx));
	}

	/// Mint new coins
	public fun mint(
		treasury_cap: &mut TreasuryCap<BURNONLY_COIN>,
		amount: u64,
		recipient: address,
		ctx: &mut TxContext,
	) {
		let coin = coin::mint<BURNONLY_COIN>(treasury_cap, amount, ctx);
		transfer::public_transfer(coin, recipient);
	}

	/// Burn coins - this will always be allowed for BurnOnly coins
	public fun burn(coin: Coin<BURNONLY_COIN>, treasury_cap: &mut TreasuryCap<BURNONLY_COIN>) {
		coin::burn(treasury_cap, coin);
	}

	/// Update coin metadata using MetadataCap
	public fun update_name(
		currency: &mut Currency<BURNONLY_COIN>,
		metadata_cap: &MetadataCap<BURNONLY_COIN>,
		new_name: vector<u8>,
	) {
		coin_registry::set_name(currency, metadata_cap, new_name.to_string());
	}

	/// Register the supply as BurnOnly, consuming the TreasuryCap
	/// After this, no more minting is allowed, but burning is still permitted
	public fun register_supply_as_burnonly(
		currency: &mut Currency<BURNONLY_COIN>,
		treasury_cap: TreasuryCap<BURNONLY_COIN>,
	) {
		coin_registry::make_supply_burn_only(currency, treasury_cap);
	}