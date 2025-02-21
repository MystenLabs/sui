// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// ===========================================================================================
/// Module: cliff
/// Description:
/// This module defines a vesting strategy in which the entire amount is vested after
/// a specific time has passed.
///
/// Functionality:
/// - Defines a cliff vesting schedule.
/// ===========================================================================================
#[allow(unused_const)]
module vesting::cliff;
use sui::coin::{Self, Coin};
use sui::clock::Clock;
use sui::balance::Balance;

// === Errors ===
#[error]
const EInvalidCliffTime: vector<u8> = b"Cliff time must be in the future.";

// === Structs ===

/// [Owned] Wallet contains coins that are available for claiming over time.
public struct Wallet<phantom T> has key, store {
    id: UID,
    // Amount of coins remaining in the wallet
    balance: Balance<T>,
    // Cliff time when the entire amount is vested
    cliff_time: u64,
    // Amount of coins that have been claimed
    claimed: u64,
}


// === Public Functions ===

/// Create a new wallet with the given coins and cliff time.
/// Note that full amount of coins is stored in the wallet when it is created;
/// it is just that the coins need to be claimable after the cliff time.
///
/// @aborts with `EInvalidCliffTime` if the cliff time is not in the future.
public fun new_wallet<T>(
    coins: Coin<T>,
    clock: &Clock,
    cliff_time: u64,
    ctx: &mut TxContext,
): Wallet<T> {
    assert!(cliff_time > clock.timestamp_ms(), EInvalidCliffTime);
    Wallet {
        id: object::new(ctx),
        balance: coins.into_balance(),
        cliff_time,
        claimed: 0
    }
}

/// Claim the coins that are available for claiming at the current time.
public fun claim<T>(
    self: &mut Wallet<T>,
    clock: &Clock,
    ctx: &mut TxContext,
): Coin<T> {
    let claimable_amount = self.claimable(clock);
    self.claimed = self.claimed + claimable_amount;
    coin::from_balance(self.balance.split(claimable_amount), ctx)
}

/// Calculate the amount of coins that can be claimed at the current time.
public fun claimable<T>(
    self: &Wallet<T>,
    clock: &Clock,
): u64 {
    let timestamp = clock.timestamp_ms();
    if (timestamp < self.cliff_time) return 0;
    self.balance.value()
}

/// Delete the wallet if it is empty.
public fun delete_wallet<T>(
    self: Wallet<T>,
) {
    let Wallet { id, balance, cliff_time: _, claimed: _ } = self;
    id.delete();
    balance.destroy_zero();
}

// === Accessors ===

/// Get the balance of the wallet.
public fun balance<T>(self: &Wallet<T>
): u64 {
    self.balance.value()
}

/// Get the cliff time of the vesting schedule.
public fun cliff_time<T>(self: &Wallet<T>
): u64 {
    self.cliff_time
}
