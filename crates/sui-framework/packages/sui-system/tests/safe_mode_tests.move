// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::safe_mode_tests {
    use sui::test_scenario;
    use sui::balance;
    use sui::test_utils;

    use sui_system::governance_test_utils::{
        create_validator_for_testing, create_sui_system_state_for_testing,
        advance_epoch_safe_mode_with_reward_amounts, advance_epoch_with_reward_amounts_return_rebate,
    };
    use sui_system::sui_system;
    use sui_system::sui_system::SuiSystemState;

    #[test]
    fun test_safe_mode_gas_accumulation() {
        let scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        {
            // First, set up the system with 4 validators.
            let ctx = test_scenario::ctx(scenario);
            create_sui_system_state_for_testing(
                vector[
                    create_validator_for_testing(@0x1, 1, ctx),
                    create_validator_for_testing(@0x2, 1, ctx),
                    create_validator_for_testing(@0x3, 1, ctx),
                    create_validator_for_testing(@0x4, 1, ctx),
                ],
                1000,
                1000,
                ctx,
            );
        };
        {
            test_scenario::next_tx(scenario, @0x1);
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            test_utils::assert_eq(sui_system::validator_stake_amount(&mut system_state, @0x1), 1);
            test_utils::assert_eq(sui_system::get_storage_fund_balance(&mut system_state), 1000);
            test_scenario::return_shared(system_state);
        };

        advance_epoch_safe_mode_with_reward_amounts(
            8,
            20,
            30,
            scenario,
        );
        let rebates = advance_epoch_with_reward_amounts_return_rebate(16, 40, 30, scenario);

        // 30 from safe mode epoch, 30 from current epoch.
        test_utils::assert_eq(balance::value(&rebates), 60);
        test_utils::destroy(rebates);

        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        // For each validator, total computation reward is 20 / 4  + 40 / 4 = 15, init stake is 1.
        // However due to integer division, each validator is getting 1 less.
        test_utils::assert_eq(sui_system::validator_stake_amount(&mut system_state, @0x1), 15);
        test_utils::assert_eq(sui_system::validator_stake_amount(&mut system_state, @0x2), 15);
        test_utils::assert_eq(sui_system::validator_stake_amount(&mut system_state, @0x3), 15);
        test_utils::assert_eq(sui_system::validator_stake_amount(&mut system_state, @0x4), 15);

        // Storage fund is 1000 + 8 + 16 - 30 - 30 = 964. 4 leftover due to integer division.
        test_utils::assert_eq(sui_system::get_storage_fund_balance(&mut system_state), 968);

        test_scenario::return_shared(system_state);
        test_scenario::end(scenario_val);
    }
}
