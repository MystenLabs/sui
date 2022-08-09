// Copyright (c) 2022, Mysten Labs, Inc.
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
        validator::new(
            addr,
            vector[204, 98, 51, 46, 52, 187, 45, 92, 214, 159, 96, 239, 187, 42, 54, 203, 145, 108, 126, 180, 88, 48, 30, 163, 102, 54, 196, 219, 176, 18, 189, 136],
            vector[204, 98, 51, 46, 52, 187, 45, 92, 214, 159, 96, 239, 187, 42, 54, 203, 145, 108, 126, 180, 88, 48, 30, 163, 102, 54, 196, 219, 176, 18, 189, 136],
            vector[126, 155, 11, 31, 209, 50, 71, 204, 26, 228, 41, 163, 34, 40, 139, 97, 119, 156, 78, 27, 22, 223, 213, 138, 164, 253, 236, 96, 109, 123, 199, 242, 48, 194, 147, 41, 197, 233, 110, 142, 159, 153, 236, 145, 245, 140, 40, 99, 104, 32, 23, 162, 120, 30, 184, 6, 77, 252, 203, 81, 243, 137, 238, 13],
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
