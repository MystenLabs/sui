// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::stake_subsidy {
    use sui::balance::{Self, Balance};
    use sui::math;
    use sui::sui::SUI;
    use sui::bag::Bag;
    use sui::bag;
    use sui::tx_context::TxContext;

    friend sui_system::genesis;
    friend sui_system::sui_system_state_inner;

    #[test_only]
    friend sui_system::governance_test_utils;

    struct StakeSubsidy has store {
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

    const BASIS_POINT_DENOMINATOR: u128 = 10000;

    const ESubsidyDecreaseRateTooLarge: u64 = 0;

    public(friend) fun create(
        balance: Balance<SUI>,
        initial_distribution_amount: u64,
        stake_subsidy_period_length: u64,
        stake_subsidy_decrease_rate: u16,
        ctx: &mut TxContext,
    ): StakeSubsidy {
        // Rate can't be higher than 100%.
        assert!(
            stake_subsidy_decrease_rate <= (BASIS_POINT_DENOMINATOR as u16),
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
    public(friend) fun advance_epoch(self: &mut StakeSubsidy): Balance<SUI> {
        // Take the minimum of the reward amount and the remaining balance in
        // order to ensure we don't overdraft the remaining stake subsidy
        // balance
        let to_withdraw = math::min(self.current_distribution_amount, balance::value(&self.balance));

        // Drawn down the subsidy for this epoch.
        let stake_subsidy = balance::split(&mut self.balance, to_withdraw);

        self.distribution_counter = self.distribution_counter + 1;

        // Decrease the subsidy amount only when the current period ends.
        if (self.distribution_counter % self.stake_subsidy_period_length == 0) {
            let decrease_amount = (self.current_distribution_amount as u128)
                * (self.stake_subsidy_decrease_rate as u128) / BASIS_POINT_DENOMINATOR;
            self.current_distribution_amount = self.current_distribution_amount - (decrease_amount as u64)
        };

        stake_subsidy
    }

    /// Returns the amount of stake subsidy to be added at the end of the current epoch.
    public fun current_epoch_subsidy_amount(self: &StakeSubsidy): u64 {
        math::min(self.current_distribution_amount, balance::value(&self.balance))
    }

    #[test_only]
    /// Returns the number of distributions that have occurred.
    public(friend) fun get_distribution_counter(self: &StakeSubsidy): u64 {
        self.distribution_counter
    }
}
