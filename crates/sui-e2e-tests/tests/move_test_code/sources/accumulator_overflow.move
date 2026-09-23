// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Test helpers that produce Move-native accumulator Merge/Split events of large amounts (up to
/// `u64::MAX`) on a balance key, used to exercise the per-key accumulator representability guards.
module move_test_code::accumulator_overflow;

use sui::balance;
use sui::coin::{Self, Coin};
use sui::object::{Self, UID};
use sui::sui::SUI;
use sui::tx_context::{Self, TxContext};

const U64_MAX: u64 = 18446744073709551615;

/// Regression helper for the old post-execution object-funds path: this attempts to withdraw
/// `u64::MAX` from a fresh object and deposit it to the sender. With in-execution object-funds
/// checking enabled, this aborts before emitting the `u64::MAX` accumulator writes.
public entry fun merge_u64_max(ctx: &mut TxContext) {
    let sender = tx_context::sender(ctx);
    let mut id = object::new(ctx);

    let w = balance::withdraw_funds_from_object<SUI>(&mut id, U64_MAX);
    let bal = balance::redeem_funds<SUI>(w);
    balance::send_funds<SUI>(bal, sender);

    object::delete(id);
}

/// Withdraw `amount` of SUI from a fresh object and return it as a `Coin<SUI>`. The per-object
/// withdrawal emits a `Split` of `amount` on that object's accumulator key, which the supply guard
/// bounds to `<= TOTAL_SUPPLY_MIST`. The returned `Coin` can be merged into `Argument::GasCoin` via
/// a PTB `MergeCoins` command — not an accumulator event — so several such withdrawals can drive the
/// gas coin's raw `u64` value up to `u64::MAX`, beyond what the supply guard permits for any single
/// balance.
public fun withdraw_sui_as_coin(amount: u64, ctx: &mut TxContext): Coin<SUI> {
    let mut id = object::new(ctx);
    let w = balance::withdraw_funds_from_object<SUI>(&mut id, amount);
    let bal = balance::redeem_funds<SUI>(w);
    object::delete(id);
    coin::from_balance<SUI>(bal, ctx)
}

/// Regression helper for a custom-coin accumulator overflow shape. With in-execution object-funds
/// checking enabled, the first unbacked withdrawal aborts before either `u64::MAX` deposit is
/// emitted.
public entry fun double_merge_u64_max<T>(ctx: &mut TxContext) {
    let sender = tx_context::sender(ctx);

    let mut id1 = object::new(ctx);
    let w1 = balance::withdraw_funds_from_object<T>(&mut id1, U64_MAX);
    balance::send_funds<T>(balance::redeem_funds<T>(w1), sender);
    object::delete(id1);

    let mut id2 = object::new(ctx);
    let w2 = balance::withdraw_funds_from_object<T>(&mut id2, U64_MAX);
    balance::send_funds<T>(balance::redeem_funds<T>(w2), sender);
    object::delete(id2);
}
