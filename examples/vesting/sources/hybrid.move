// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// ===========================================================================================
/// Module: hybrid
/// Description:
/// This module defines a vesting strategy in which half of the tokens are cliff vested,
/// and the other half are linearly vested.
///
/// Functionality:
/// - Defines a hybrid vesting schedule.
/// ===========================================================================================
#[allow(unused_const)]
module vesting::hybrid;

use vesting::cliff;
use vesting::linear;
use sui::coin::{Self, Coin};
use sui::clock::Clock;


// === Structs ===

/// [Owned] Wallet contains coins that are available for claiming over time.
public struct Wallet<phantom T> has key, store {
    id: UID,
    // A wallet that uses cliff vesting for the first half of the balance
    cliff_vested: cliff::Wallet<T>,
    // A wallet that uses linear vesting for the second half of the balance
    linear_vested: linear::Wallet<T>,
}


// === Public Functions ===

/// Create a new wallet with the given coins and vesting duration.
/// Note that full amount of coins is stored in the wallet when it is created;
/// it is just that the coins need to be claimed over time.
/// The first half of the coins are cliff vested, which takes start_cliff time to vest.
/// The second half of the coins are linearly vested, which starts at start_linear time
public fun new_wallet<T>(
    coins: Coin<T>,
    clock: &Clock,
    start_cliff: u64,
    start_linear: u64,
    duration_linear: u64,
    ctx: &mut TxContext,
): Wallet<T> {
    let mut balance = coins.into_balance();
    let balance_cliff = balance.value() * 50 / 100;
    Wallet {
        id: object::new(ctx),
        cliff_vested: cliff::new_wallet(
            coin::from_balance(balance.split(balance_cliff), ctx),
            clock,
            start_cliff,
            ctx,
        ),
        linear_vested: linear::new_wallet(
            coin::from_balance(balance, ctx),
            clock,
            start_linear,
            duration_linear,
            ctx,
        ),
    }
}

/// Claim the coins that are available for claiming at the current time.
public fun claim<T>(
    self: &mut Wallet<T>,
    clock: &Clock,
    ctx: &mut TxContext,
): Coin<T> {
    let mut coin_cliff = self.cliff_vested.claim(clock, ctx);
    let coin_linear = self.linear_vested.claim(clock, ctx);
    coin_cliff.join(coin_linear);
    coin_cliff
}

/// Calculate the amount of coins that can be claimed at the current time.
public fun claimable<T>(
    self: &Wallet<T>,
    clock: &Clock,
): u64 {
    self.cliff_vested.claimable(clock) + self.linear_vested.claimable(clock)
}

/// Delete the wallet if it is empty.
public fun delete_wallet<T>(
    self: Wallet<T>,
) {
    let Wallet {
        id,
        cliff_vested,
        linear_vested,
    } = self;
    cliff_vested.delete_wallet();
    linear_vested.delete_wallet();
    id.delete();
}

// === Accessors ===

/// Get the balance of the wallet.
public fun balance<T>(self: &Wallet<T>
): u64 {
    self.cliff_vested.balance() + self.linear_vested.balance()
}
