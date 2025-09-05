// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::stake_subsidy;

use sui::bag::{Self, Bag};
use sui::balance::Balance;
use sui::sui::SUI;

const ESubsidyDecreaseRateTooLarge: u64 = 0;

const BASIS_POINT_DENOMINATOR: u128 = 100_00;

public struct StakeSubsidy has store {
    /// Balance of SUI set aside for stake subsidies that will be drawn down over time.
    balance: Balance<SUI>,
    /// Count of the number of times stake subsidies have been distributed.
    distribution_counter: u64,
    /// The amount of stake subsidy to be drawn down per distribution.
    /// This amount decays and decreases over time.
    current_distribution_amount: u64,
    /// Number of distributions to occur before the distribution amount decays.
    stake_subsidy_period_length: u64,
    /// The rate at which the distribution amount decays at the end of each
    /// period. Expressed in basis points.
    stake_subsidy_decrease_rate: u16,
    /// Any extra fields that's not defined statically.
    extra_fields: Bag,
}

public(package) fun create(
    balance: Balance<SUI>,
    initial_distribution_amount: u64,
    stake_subsidy_period_length: u64,
    stake_subsidy_decrease_rate: u16,
    ctx: &mut TxContext,
): StakeSubsidy {
    // Rate can't be higher than 100%.
    assert!(
        stake_subsidy_decrease_rate <= BASIS_POINT_DENOMINATOR as u16,
        ESubsidyDecreaseRateTooLarge,
    );

    StakeSubsidy {
        balance,
        distribution_counter: 0,
        current_distribution_amount: initial_distribution_amount,
        stake_subsidy_period_length,
        stake_subsidy_decrease_rate,
        extra_fields: bag::new(ctx),
    }
}

/// Advance the epoch counter and draw down the subsidy for the epoch.
public(package) fun advance_epoch(self: &mut StakeSubsidy): Balance<SUI> {
    // Take the minimum of the reward amount and the remaining balance in
    // order to ensure we don't overdraft the remaining stake subsidy
    // balance
    let to_withdraw = self.current_distribution_amount.min(self.balance.value());

    // Drawn down the subsidy for this epoch.
    let stake_subsidy = self.balance.split(to_withdraw);
    self.distribution_counter = self.distribution_counter + 1;

    // Decrease the subsidy amount only when the current period ends.
    if (self.distribution_counter % self.stake_subsidy_period_length == 0) {
        let decrease_amount =
            self.current_distribution_amount as u128
            * (self.stake_subsidy_decrease_rate as u128) / BASIS_POINT_DENOMINATOR;

        self.current_distribution_amount =
            self.current_distribution_amount - (decrease_amount as u64)
    };

    stake_subsidy
}

/// Returns the amount of stake subsidy to be added at the end of the current epoch.
public fun current_epoch_subsidy_amount(self: &StakeSubsidy): u64 {
    self.current_distribution_amount.min(self.balance.value())
}

/// Returns the number of distributions that have occurred.
public(package) fun get_distribution_counter(self: &StakeSubsidy): u64 {
    self.distribution_counter
}

#[test_only]
public(package) fun set_distribution_counter(self: &mut StakeSubsidy, distribution_counter: u64) {
    self.distribution_counter = distribution_counter;
}
