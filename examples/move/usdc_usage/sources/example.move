// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module usdc_usage::example;

use sui::coin::Coin;
use sui::sui::SUI;
use usdc::usdc::USDC;

public struct Sword has key, store {
    id: UID,
    strength: u64,
}

public fun buy_sword_with_usdc(coin: Coin<USDC>, ctx: &mut TxContext): Sword {
    let sword = create_sword(coin.value(), ctx);
    // In production: transfer to actual recipient! Don't transfer to 0x0!
    transfer::public_transfer(coin, @0x0);
    sword
}

public fun buy_sword_with_sui(coin: Coin<SUI>, ctx: &mut TxContext): Sword {
    let sword = create_sword(coin.value(), ctx);
    // In production: transfer to actual recipient! Don't transfer to 0x0!
    transfer::public_transfer(coin, @0x0);
    sword
}

public fun buy_sword_with_arbitrary_coin<CoinType>(
    coin: Coin<CoinType>,
    ctx: &mut TxContext,
): Sword {
    let sword = create_sword(coin.value(), ctx);
    // In production: transfer to actual recipient! Don't transfer to 0x0!
    transfer::public_transfer(coin, @0x0);
    sword
}

/// A helper function to create a sword.
fun create_sword(strength: u64, ctx: &mut TxContext): Sword {
    let id = object::new(ctx);
    Sword { id, strength }
}
