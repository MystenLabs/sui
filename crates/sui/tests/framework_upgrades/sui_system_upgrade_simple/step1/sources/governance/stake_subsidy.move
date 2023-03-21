// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::stake_subsidy {
    use sui::balance::Balance;
    use sui::sui::SUI;
    use sui::bag::Bag;
    use sui::bag;
    use sui::tx_context::TxContext;

    friend sui::sui_system_state_inner;

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

    public(friend) fun create(
        balance: Balance<SUI>,
        initial_stake_subsidy_amount: u64,
        stake_subsidy_period_length: u64,
        stake_subsidy_decrease_rate: u16,
        ctx: &mut TxContext,
    ): StakeSubsidy {
        // Rate can't be higher than 100%.
        assert!(
            stake_subsidy_decrease_rate <= (BASIS_POINT_DENOMINATOR as u16),
            0,
        );

        StakeSubsidy {
            balance,
            distribution_counter: 0,
            current_distribution_amount: initial_stake_subsidy_amount,
            stake_subsidy_period_length,
            stake_subsidy_decrease_rate,
            extra_fields: bag::new(ctx),
        }
    }
}
