// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// ===========================================================================================
/// Module: backloaded
/// Description:
/// This module defines a vesting strategy in which the majority amount is vested
/// near the end of a vesting period.
/// The vesting schedule is split into two portions: the front portion and the back portion.
/// Each portion is implemented using linear vesting schedules.
///
/// Functionality:
/// - Defines a backloaded vesting schedule.
/// ===========================================================================================
#[allow(unused_const)]
module vesting::backloaded;

use vesting::linear;
use sui::coin::{Self, Coin};
use sui::clock::Clock;

// === Errors ===
#[error]
const EInvalidBackStartTime: vector<u8> = b"Start time of back portion must be after front portion.";
#[error]
const EInvalidPercentageRange: vector<u8> = b"Percentage range must be between 50 to 100.";
#[error]
const EInsufficientBalance: vector<u8> = b"Not enough balance for vesting.";
#[error]
const EInvalidDuration: vector<u8> = b"Duration must be long enough to complete back portion.";

// === Structs ===

/// [Owned] Wallet contains coins that are available for claiming over time.
public struct Wallet<phantom T> has key, store {
    id: UID,
    // A wallet that stores the front (first) portion of the balance
    front: linear::Wallet<T>,
    // A wallet that stores the back (last) portion of the balance
    back: linear::Wallet<T>,
    // Time when the vesting started
    start_front: u64,
    // Time when the back portion of the vesting started; start_front < start_back
    start_back: u64,
    // Total duration of the vesting schedule
    duration: u64,
    // Percentage of balance that is vested in the back portion; value is between 50 and 100
    back_percentage: u8,
}


// === Public Functions ===

/// Create a new wallet with the given coins and vesting duration.
/// Full amount of coins is stored in the wallet when it is created;
/// but the coins are claimed over time.
///
/// When the front portion is vested over a short period of time
/// such that `duration - start_back > start_back - start_front`, then
/// more coins might be claimed in the front portion than the back portion.
/// To prevent this case, make sure that the back portion has higher percentage of the balance
/// via `back_percentage`.
///
/// @aborts with `EInvalidBackStartTime` if the back start time is before the front start time.
/// @aborts with `EInvalidPercentageRange` if the percentage range is not between 50 to 100.
/// @aborts with `EInvalidDuration` if the duration is not long enough to complete the back portion.
/// @aborts with `EInsufficientBalance` if the balance is not enough to split into front and back portions.
public fun new_wallet<T>(
    coins: Coin<T>,
    clock: &Clock,
    start_front: u64,
    start_back: u64,
    duration: u64,
    back_percentage: u8,
    ctx: &mut TxContext,
): Wallet<T> {
    assert!(start_back > start_front, EInvalidBackStartTime);
    assert!(back_percentage > 50 && back_percentage <= 100, EInvalidPercentageRange);
    assert!(duration > start_back - start_front, EInvalidDuration);
    let mut balance = coins.into_balance();
    let balance_back = balance.value() * (back_percentage as u64) / 100;
    let balance_front = balance.value() - balance_back;
    assert!(balance_front > 0 && balance_back > 0, EInsufficientBalance);
    Wallet {
        id: object::new(ctx),
        front: linear::new_wallet(
            coin::from_balance(balance.split(balance_front), ctx),
            clock,
            start_front,
            start_back - start_front,
            ctx,
        ),
        back: linear::new_wallet(
            coin::from_balance(balance, ctx),
            clock,
            start_back,
            duration - (start_back - start_front),
            ctx,
        ),
        start_front,
        start_back,
        duration,
        back_percentage,
    }
}

/// Claim the coins that are available for claiming at the current time.
public fun claim<T>(
    self: &mut Wallet<T>,
    clock: &Clock,
    ctx: &mut TxContext,
): Coin<T> {
    let mut coin_front = self.front.claim(clock, ctx);
    let coin_back = self.back.claim(clock, ctx);
    coin_front.join(coin_back);
    coin_front
}

/// Calculate the amount of coins that can be claimed at the current time.
public fun claimable<T>(
    self: &Wallet<T>,
    clock: &Clock,
): u64 {
    self.front.claimable(clock) + self.back.claimable(clock)
}

/// Delete the wallet if it is empty.
public fun delete_wallet<T>(
    self: Wallet<T>,
) {
    let Wallet {
        id,
        front,
        back,
        start_front: _,
        start_back: _,
        duration: _,
        back_percentage: _,
    } = self;
    front.delete_wallet();
    back.delete_wallet();
    id.delete();
}

// === Accessors ===

/// Get the balance of the wallet.
public fun balance<T>(self: &Wallet<T>
): u64 {
    self.front.balance() + self.back.balance()
}

/// Get the start time of the vesting schedule.
public fun start<T>(self: &Wallet<T>
): u64 {
    self.start_front
}

/// Get the duration of the vesting schedule.
public fun duration<T>(self: &Wallet<T>
): u64 {
    self.duration
}
