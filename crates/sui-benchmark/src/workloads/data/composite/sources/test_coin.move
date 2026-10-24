// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A regulated test coin for multi-currency stress testing.
module basics::test_coin;

use sui::coin::{Self, TreasuryCap, Coin, DenyCapV2};
use sui::balance::Balance;
use sui::deny_list::DenyList;

public struct TEST_COIN has drop {}

public struct TestCoinCap has key, store {
    id: UID,
    treasury: TreasuryCap<TEST_COIN>,
}

#[allow(deprecated_usage)]
fun init(witness: TEST_COIN, ctx: &mut TxContext) {
    let (treasury, deny_cap, metadata) = coin::create_regulated_currency_v2(
        witness,
        9,
        b"TEST",
        b"Test Coin",
        b"A test coin for stress testing",
        option::none(),
        false,
        ctx,
    );
    transfer::public_transfer(metadata, ctx.sender());
    transfer::public_transfer(treasury, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
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

public fun deny(
    deny_list: &mut DenyList,
    cap: &mut DenyCapV2<TEST_COIN>,
    addr: address,
    ctx: &mut TxContext,
) {
    coin::deny_list_v2_add(deny_list, cap, addr, ctx);
}

public fun undeny(
    deny_list: &mut DenyList,
    cap: &mut DenyCapV2<TEST_COIN>,
    addr: address,
    ctx: &mut TxContext,
) {
    coin::deny_list_v2_remove(deny_list, cap, addr, ctx);
}
