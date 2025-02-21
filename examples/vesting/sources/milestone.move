// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// ===========================================================================================
/// Module: milestone
/// Description:
/// This module defines a vesting strategy that allows users to claim coins
/// as the milestones are achieved.
///
/// Functionality:
/// - Defines a milestone-based vesting schedule.
/// ===========================================================================================
module vesting::milestone;
use sui::coin::{Self, Coin};
use sui::balance::Balance;

// === Errors ===
#[error]
const EOwnerIsController: vector<u8> = b"Owner cannot be the milestone controller.";
#[error]
const EUnauthorizedOwner: vector<u8> = b"Unauthorized owner.";
#[error]
const EUnauthorizedMilestoneController: vector<u8> = b"Unauthorized milestone controller.";
#[error]
const EMilestonePercentageRange: vector<u8> = b"Invalid milestone percentage.";
#[error]
const EInvalidNewMilestone: vector<u8> = b"New milestone must be greater than the current milestone.";

// === Structs ===

/// [Shared] Wallet contains coins that are available for claiming.
public struct Wallet<phantom T> has key, store {
    id: UID,
    // Amount of coins remaining in the wallet
    balance: Balance<T>,
    // Amount of coins that have been claimed
    claimed: u64,
    // Achieved milestone in percentage from 0 to 100
    milestone_percentage: u8,
    // Owner of the wallet
    owner: address,
    // Milestone controller of the wallet
    milestone_controller: address,
}


// === Public Functions ===

/// Create a new wallet with the given coins and vesting duration.
/// Note that full amount of coins is stored in the wallet when it is created;
/// it is just that the coins need to be claimed as the milestones are achieved.
///
/// @aborts with `EOwnerIsController` if the owner is same as the milestone controller.
public fun new_wallet<T>(
    coins: Coin<T>,
    owner: address,
    milestone_controller: address,
    ctx: &mut TxContext,
) {
    assert!(owner != milestone_controller, EOwnerIsController);
    let wallet = Wallet {
        id: object::new(ctx),
        balance: coins.into_balance(),
        claimed: 0,
        milestone_percentage: 0,
        owner,
        milestone_controller,
    };
    transfer::share_object(wallet);
}

/// Claim the coins that are available based on the current milestone.
///
/// @aborts with `EUnauthorizedUser` if the sender is not the owner of the wallet.
public fun claim<T>(
    self: &mut Wallet<T>,
    ctx: &mut TxContext,
): Coin<T> {
    assert!(self.owner == ctx.sender(), EUnauthorizedOwner);
    let claimable_amount = self.claimable();
    self.claimed = self.claimed + claimable_amount;
    coin::from_balance(self.balance.split(claimable_amount), ctx)
}

/// Calculate the current amount of coins that can be claimed.
public fun claimable<T>(
    self: &Wallet<T>,
): u64 {
    // Convert the balance to u128 to account for overflow in the calculation
    let claimable: u128 = (self.balance.value() + self.claimed as u128) * (self.milestone_percentage as u128) / 100;
    // Adjust the claimable amount by subtracting the already claimed amount
    (claimable as u64) - self.claimed
}

/// Update the milestone percentage of the wallet.
///
/// @aborts with `EUnauthorizedMilestoneController` if the sender is not the milestone controller.
/// @aborts with `EMilestonePercentageRange` if the new milestone percentage is invalid.
/// @aborts with `EInvalidNewMilestone` if the new milestone is not greater than the current milestone.
public fun update_milestone_percentage<T>(
    self: &mut Wallet<T>,
    percentage: u8,
    ctx: &mut TxContext,
) {
    assert!(self.milestone_controller == ctx.sender(), EUnauthorizedMilestoneController);
    assert!(percentage > 0 && percentage <= 100, EMilestonePercentageRange);
    assert!(percentage > self.milestone_percentage, EInvalidNewMilestone);
    self.milestone_percentage = percentage;
}

/// Delete the wallet if it is empty.
public fun delete_wallet<T>(
    self: Wallet<T>,
) {
    let Wallet { id, balance, claimed: _, milestone_percentage: _, owner: _, milestone_controller: _ } = self;
    id.delete();
    balance.destroy_zero();
}

// === Accessors ===

/// Get the remaining balance of the wallet.
public fun balance<T>(self: &Wallet<T>
): u64 {
    self.balance.value()
}

/// Get the start time of the vesting schedule.
public fun milestone<T>(self: &Wallet<T>
): u8 {
    self.milestone_percentage
}

/// Get the owner of the wallet.
public fun get_owner<T>(self: &Wallet<T>
): address {
    self.owner
}

/// Get the milestone controller of the wallet.
public fun get_milestone_controller<T>(self: &Wallet<T>
): address {
    self.milestone_controller
}
