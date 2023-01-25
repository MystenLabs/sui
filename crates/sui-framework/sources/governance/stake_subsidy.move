// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::stake_subsidy {
    use sui::balance::{Self, Balance, Supply};
    use sui::sui::SUI;

    friend sui::sui_system;

    struct StakeSubsidy has store {
        /// This counter may be different from the current epoch number if
        /// in some epochs we decide to skip the subsidy. 
        epoch_counter: u64,
        /// Balance storing the accumulated stake subsidy.
        balance: Balance<SUI>,
        /// The amount of stake subsidy to be minted this epoch.
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

    public(friend) fun mint_stake_subsidy_proportional_to_total_stake_testnet(
        subsidy: &mut StakeSubsidy, supply: &mut Supply<SUI>, stake_subsidy_rate: u64, total_stake: u64
    ) {
        let amount_to_mint = ((total_stake as u128) * (stake_subsidy_rate as u128)) / BASIS_POINT_DENOMINATOR;
        balance::join(
            &mut subsidy.balance, 
            balance::increase_supply(supply, (amount_to_mint as u64))
        );
    }

    /// Advance the epoch counter and mint new subsidy for the epoch.
    public(friend) fun advance_epoch(subsidy: &mut StakeSubsidy, supply: &mut Supply<SUI>) {
        // Mint new subsidy for this epoch.
        balance::join(
            &mut subsidy.balance, 
            balance::increase_supply(supply, subsidy.current_epoch_amount)
        );
        subsidy.epoch_counter = subsidy.epoch_counter + 1;
        // Decrease the subsidy amount only when the current period ends.
        if (subsidy.epoch_counter % STAKE_SUBSIDY_PERIOD_LENGTH == 0) {
            let decrease_amount = (subsidy.current_epoch_amount as u128)
                * STAKE_SUBSIDY_DECREASE_RATE / BASIS_POINT_DENOMINATOR;
            subsidy.current_epoch_amount = subsidy.current_epoch_amount - (decrease_amount as u64)
        };
    }

    /// Withdraw all the minted stake subsidy.
    public(friend) fun withdraw_all(subsidy: &mut StakeSubsidy): Balance<SUI> {
        let amount = balance::value(&subsidy.balance);
        balance::split(&mut subsidy.balance, amount)
    }

    /// Returns the amount of stake subsidy to be added at the end of the current epoch.
    public fun current_epoch_subsidy_amount(subsidy: &StakeSubsidy): u64 {
        subsidy.current_epoch_amount
    }
}
