// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example regulated coin using create_regulated_currency_v3 API
module regulated_coin::regulated_coin {
	use sui::{
		coin::{Self, TreasuryCap, DenyCapV2},
		coin_registry::{Self, MetadataCap},
		deny_list,
		transfer,
		tx_context::{Self, TxContext}
	};

	/// Name of the coin
	public struct REGULATED_COIN has drop {}

	/// Register the regulated currency using create_regulated_currency_v3 API
	fun init(witness: REGULATED_COIN, ctx: &mut TxContext) {
		let (treasury_cap, metadata_cap, deny_cap, init_coin_data) = coin::create_regulated_currency_v3<
			REGULATED_COIN,
		>(
			witness,
			9, // decimals
			b"REG".to_string(),
			b"Regulated Coin".to_string(),
			b"Regulated coin for testing GetCoinInfo with CoinRegistry".to_string(),
			b"https://example.com/regulated.png".to_string(),
			true, // allow_global_pause
			ctx,
		);

		// Transfer to CoinRegistry
		coin_registry::transfer_to_registry(init_coin_data);

		// Transfer caps to sender
		transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
		transfer::public_transfer(metadata_cap, tx_context::sender(ctx));
		transfer::public_transfer(deny_cap, tx_context::sender(ctx));
	}

	/// Mint new coins
	public entry fun mint(
		treasury_cap: &mut TreasuryCap<REGULATED_COIN>,
		amount: u64,
		recipient: address,
		ctx: &mut TxContext,
	) {
		let coin = coin::mint<REGULATED_COIN>(treasury_cap, amount, ctx);
		transfer::public_transfer(coin, recipient);
	}

	/// Add an address to the deny list
	public entry fun deny_address(
		deny_list: &mut deny_list::DenyList,
		deny_cap: &mut DenyCapV2<REGULATED_COIN>,
		address_to_deny: address,
		ctx: &mut TxContext,
	) {
		coin::deny_list_v2_add(deny_list, deny_cap, address_to_deny, ctx);
	}
}

