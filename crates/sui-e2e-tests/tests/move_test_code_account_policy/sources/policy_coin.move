// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A coin type other than SUI, for exercising per-type account policy limits.
module move_test_code_account_policy::policy_coin;

use sui::coin::{Self, Coin, TreasuryCap};

public struct POLICY_COIN has drop {}

fun init(witness: POLICY_COIN, ctx: &mut TxContext) {
    let (cap, metadata) = coin::create_currency(
        witness,
        6,
        b"PC",
        b"PolicyCoin",
        b"",
        option::none(),
        ctx,
    );
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(cap, ctx.sender())
}

public fun mint(cap: &mut TreasuryCap<POLICY_COIN>, amount: u64, ctx: &mut TxContext): Coin<POLICY_COIN> {
    cap.mint(amount, ctx)
}
