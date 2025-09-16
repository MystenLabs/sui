// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example coin using the legacy create_currency API (v1)
#[allow(deprecated_usage)]
module legacy_coin::legacy_coin;

use std::option;
use sui::{coin::{Self, TreasuryCap}, transfer, tx_context::{Self, TxContext}, url};

	/// Name of the coin
	public struct LEGACY_COIN has drop {}

	/// Register the legacy currency using create_currency v1 API
	fun init(witness: LEGACY_COIN, ctx: &mut TxContext) {
		let (treasury_cap, metadata) = coin::create_currency<LEGACY_COIN>(
			witness,
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
		transfer::public_transfer(treasury_cap, tx_context::sender(ctx))
	}

	/// Mint new coins
	public fun mint(
		treasury_cap: &mut TreasuryCap<LEGACY_COIN>,
		amount: u64,
		recipient: address,
		ctx: &mut TxContext,
	) {
		let coin = coin::mint<LEGACY_COIN>(treasury_cap, amount, ctx);
		transfer::public_transfer(coin, recipient);
	}