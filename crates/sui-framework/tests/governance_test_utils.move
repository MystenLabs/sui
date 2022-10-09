// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::governance_test_utils {
    use sui::balance;
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::test_scenario::{Self, Scenario};
    use std::option;

    public fun create_validator_for_testing(
        addr: address, init_stake_amount: u64, ctx: &mut TxContext
    ): Validator {
        validator::new_for_testing(
            addr,
            x"FF",
            x"FF",
            x"FF",
            b"ValidatorName",
            x"FFFF",
            balance::create_for_testing<SUI>(init_stake_amount),
            option::none(),
            1,
            ctx
        )
    }

    public fun create_sui_system_state_for_testing(
        validators: vector<Validator>, sui_supply_amount: u64, storage_fund_amount: u64
    ) {
        sui_system::create(
            validators,
            balance::create_supply_for_testing(sui_supply_amount), // sui_supply
            balance::create_for_testing<SUI>(storage_fund_amount), // storage_fund
            1024, // max_validator_candidate_count
            0, // min_validator_stake
            1, //storage_gas_price
        )
    }

    public fun advance_epoch(state: &mut SuiSystemState, scenario: &mut Scenario) {
        test_scenario::next_epoch(scenario);
        let new_epoch = tx_context::epoch(test_scenario::ctx(scenario));
        sui_system::advance_epoch(state, new_epoch, 0, 0, &mut tx_context::dummy());
    }
}
