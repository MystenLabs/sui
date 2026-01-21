// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A test coin for multi-currency stress testing.
module basics::test_coin;

use sui::coin::{Self, TreasuryCap, Coin};
use sui::balance::Balance;

public struct TEST_COIN has drop {}

public struct TestCoinCap has key, store {
    id: UID,
    treasury: TreasuryCap<TEST_COIN>,
}

#[allow(deprecated_usage)]
fun init(witness: TEST_COIN, ctx: &mut TxContext) {
    let (treasury, metadata) = coin::create_currency(
        witness,
        9,
        b"TEST",
        b"Test Coin",
        b"A test coin for stress testing",
        option::none(),
        ctx,
    );
    transfer::public_transfer(metadata, ctx.sender());
    transfer::public_transfer(treasury, ctx.sender());
}

public fun create_cap(treasury: TreasuryCap<TEST_COIN>, ctx: &mut TxContext) {
    transfer::share_object(TestCoinCap {
        id: object::new(ctx),
        treasury,
    });
}

public fun mint(cap: &mut TestCoinCap, amount: u64, ctx: &mut TxContext): Coin<TEST_COIN> {
    coin::mint(&mut cap.treasury, amount, ctx)
}

public fun mint_balance(cap: &mut TestCoinCap, amount: u64): Balance<TEST_COIN> {
    coin::mint_balance(&mut cap.treasury, amount)
}

public fun burn(cap: &mut TestCoinCap, coin: Coin<TEST_COIN>) {
    coin::burn(&mut cap.treasury, coin);
}
