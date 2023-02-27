// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::stake_subsidy {
    use sui::balance::{Self, Balance};
    use sui::math;
    use sui::sui::SUI;

    friend sui::sui_system;

    struct StakeSubsidy has store {
        /// This counter may be different from the current epoch number if
        /// in some epochs we decide to skip the subsidy. 
        epoch_counter: u64,
        /// Balance of Sui set asside for Staking subsidies that will be drawn down over time.
        balance: Balance<SUI>,
        /// The amount of stake subsidy to be drawn down per epoch.
        /// This amount decays and decreases over time.
        current_epoch_amount: u64,
    }

    const BASIS_POINT_DENOMINATOR: u128 = 10000;

    // Placeholder numbers.
    const STAKE_SUBSIDY_DECREASE_RATE: u128 = 1000; // in basis point
    const STAKE_SUBSIDY_PERIOD_LENGTH: u64 = 30; // in number of epochs

    public(friend) fun create(initial_stake_subsidy_amount: u64): StakeSubsidy {
        StakeSubsidy {
            epoch_counter: 0,
            balance: balance::zero(),
            current_epoch_amount: initial_stake_subsidy_amount,
        }
    }

    /// Advance the epoch counter and draw down the subsidy for the epoch.
    public(friend) fun advance_epoch(subsidy: &mut StakeSubsidy): Balance<SUI> {
        // Take the minimum of the reward amount and the remaining balance in
        // order to ensure we don't overdraft the remaining stake subsidy
        // balance
        let to_withdrawl = math::min(subsidy.current_epoch_amount, balance::value(&subsidy.balance));

        // Drawn down the subsidy for this epoch.
        let stake_subsidy = balance::split(&mut subsidy.balance, to_withdrawl);

        subsidy.epoch_counter = subsidy.epoch_counter + 1;

        // Decrease the subsidy amount only when the current period ends.
        if (subsidy.epoch_counter % STAKE_SUBSIDY_PERIOD_LENGTH == 0) {
            let decrease_amount = (subsidy.current_epoch_amount as u128)
                * STAKE_SUBSIDY_DECREASE_RATE / BASIS_POINT_DENOMINATOR;
            subsidy.current_epoch_amount = subsidy.current_epoch_amount - (decrease_amount as u64)
        };

        stake_subsidy
    }

    /// Returns the amount of stake subsidy to be added at the end of the current epoch.
    public fun current_epoch_subsidy_amount(subsidy: &StakeSubsidy): u64 {
        subsidy.current_epoch_amount
    }
}
