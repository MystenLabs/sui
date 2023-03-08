// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::validator_set_tests {
    use sui::balance;
    use sui::coin;
    use sui::tx_context::TxContext;
    use sui::validator::{Self, Validator, staking_pool_id};
    use sui::validator_set::{Self, ValidatorSet};
    use sui::test_scenario::{Self, Scenario};
    use sui::vec_map;
    use std::ascii;
    use std::option;
    use sui::test_utils::{Self, assert_eq};

    #[test]
    fun test_validator_set_flow() {
        let scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = test_scenario::ctx(scenario);

        // Create 4 validators, with stake 100, 200, 300, 400. Only the first validator is an initial validator.
        let validator1 = create_validator(@0x1, 1, 1, true, ctx1);
        let validator2 = create_validator(@0x2, 2, 1, false, ctx1);
        let validator3 = create_validator(@0x3, 3, 1, false, ctx1);
        let validator4 = create_validator(@0x4, 4, 1, false, ctx1);

        // Create a validator set with only the first validator in it.
        let validator_set = validator_set::new(vector[validator1], ctx1);
        assert!(validator_set::total_stake(&validator_set) == 100, 0);

        // Add the other 3 validators one by one.
        add_and_activate_validator(
            &mut validator_set,
            validator2,
            scenario
        );
        // Adding validator during the epoch should not affect stake and quorum threshold.
        assert!(validator_set::total_stake(&validator_set) == 100, 0);

        add_and_activate_validator(
            &mut validator_set,
            validator3,
            scenario
        );

        test_scenario::next_tx(scenario, @0x1);
        {
            let ctx1 = test_scenario::ctx(scenario);
            validator_set::request_add_stake(
                &mut validator_set,
                @0x1,
                coin::into_balance(coin::mint_for_testing(500, ctx1)),
                ctx1,
            );
            // Adding stake to existing active validator during the epoch
            // should not change total stake.
            assert!(validator_set::total_stake(&validator_set) == 100, 0);
        };

        add_and_activate_validator(
            &mut validator_set,
            validator4,
            scenario
        );

        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);
        // Total stake for these should be the starting stake + the 500 staked with validator 1 in addition to the starting stake.
        assert!(validator_set::total_stake(&validator_set) == 1500, 0);

        test_scenario::next_tx(scenario, @0x1);
        {
            let ctx1 = test_scenario::ctx(scenario);

            validator_set::request_remove_validator(
                &mut validator_set,
                ctx1,
            );
        };

        // Total validator candidate count changes, but total stake remains during epoch.
        assert!(validator_set::total_stake(&validator_set) == 1500, 0);
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);
        // Validator1 is gone. This removes its stake (100) + the 500 staked with it.
        assert!(validator_set::total_stake(&validator_set) == 900, 0);

        test_utils::destroy(validator_set);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_reference_gas_price_derivation() {
        let scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = test_scenario::ctx(scenario);
        // Create 5 validators with different stakes and different gas prices.
        let v1 = create_validator(@0x1, 1, 45, true, ctx1);
        let v2 = create_validator(@0x2, 2, 42, false, ctx1);
        let v3 = create_validator(@0x3, 3, 40, false, ctx1);
        let v4 = create_validator(@0x4, 4, 41, false, ctx1);
        let v5 = create_validator(@0x5, 10, 43, false, ctx1);

        // Create a validator set with only the first validator in it.
        let validator_set = validator_set::new(vector[v1], ctx1);

        assert_eq(validator_set::derive_reference_gas_price(&validator_set), 45);

        add_and_activate_validator(&mut validator_set, v2, scenario);
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);

        assert_eq(validator_set::derive_reference_gas_price(&validator_set), 45);

        add_and_activate_validator(
            &mut validator_set,
            v3,
            scenario
        );
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);

        assert_eq(validator_set::derive_reference_gas_price(&validator_set), 42);

        add_and_activate_validator(
            &mut validator_set,
            v4,
            scenario
        );
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);

        assert_eq(validator_set::derive_reference_gas_price(&validator_set), 42);

        add_and_activate_validator(
            &mut validator_set,
            v5,
            scenario
        );
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);

        assert_eq(validator_set::derive_reference_gas_price(&validator_set), 43);

        test_utils::destroy(validator_set);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = validator_set::EMinJoiningStakeNotReached)]
    fun test_add_validator_failure_below_min_stake() {
        let scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = test_scenario::ctx(scenario);

        // Create 2 validators, with stake 100 and 200.
        let validator1 = create_validator(@0x1, 1, 1, true, ctx1);
        let validator2 = create_validator(@0x2, 2, 1, false, ctx1);

        // Create a validator set with only the first validator in it.
        let validator_set = validator_set::new(vector[validator1], ctx1);
        assert_eq(validator_set::total_stake(&validator_set), 100);

        validator_set::request_add_validator_candidate(&mut validator_set, validator2);

        test_scenario::next_tx(scenario, @0x42);
        {
            let ctx = test_scenario::ctx(scenario);
            validator_set::request_add_stake(
                &mut validator_set,
                @0x2,
                balance::create_for_testing(500),
                ctx,
            );
            // Adding stake to a preactive validator should not change total stake.
            assert_eq(validator_set::total_stake(&validator_set), 100);
        };

        test_scenario::next_tx(scenario, @0x2);
        // Validator 2 now has 700 MIST in stake but that's not enough because we need 701.
        validator_set::request_add_validator(&mut validator_set, 701, test_scenario::ctx(scenario));

        test_utils::destroy(validator_set);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_add_validator_with_nonzero_min_stake() {
        let scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = test_scenario::ctx(scenario);

        // Create 2 validators, with stake 100 and 200.
        let validator1 = create_validator(@0x1, 1, 1, true, ctx1);
        let validator2 = create_validator(@0x2, 2, 1, false, ctx1);

        // Create a validator set with only the first validator in it.
        let validator_set = validator_set::new(vector[validator1], ctx1);
        assert_eq(validator_set::total_stake(&validator_set), 100);

        validator_set::request_add_validator_candidate(&mut validator_set, validator2);

        test_scenario::next_tx(scenario, @0x42);
        {
            let ctx = test_scenario::ctx(scenario);
            validator_set::request_add_stake(
                &mut validator_set,
                @0x2,
                balance::create_for_testing(500),
                ctx,
            );
            // Adding stake to a preactive validator should not change total stake.
            assert_eq(validator_set::total_stake(&validator_set), 100);
        };

        test_scenario::next_tx(scenario, @0x2);
        // Validator 2 now has 700 MIST in stake and that's just enough.
        validator_set::request_add_validator(&mut validator_set, 700, test_scenario::ctx(scenario));

        test_utils::destroy(validator_set);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_add_candidate_then_remove() {
        let scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = test_scenario::ctx(scenario);

        // Create 2 validators, with stake 100 and 200.
        let validator1 = create_validator(@0x1, 1, 1, true, ctx1);
        let validator2 = create_validator(@0x2, 2, 1, false, ctx1);

        let pool_id_2 = staking_pool_id(&validator2);

        // Create a validator set with only the first validator in it.
        let validator_set = validator_set::new(vector[validator1], ctx1);
        assert_eq(validator_set::total_stake(&validator_set), 100);

        // Add the second one as a candidate.
        validator_set::request_add_validator_candidate(&mut validator_set, validator2);
        assert!(validator_set::is_validator_candidate(&validator_set, @0x2), 0);

        test_scenario::next_tx(scenario, @0x2);
        // Then remove its candidacy.
        validator_set::request_remove_validator_candidate(&mut validator_set, test_scenario::ctx(scenario));
        assert!(!validator_set::is_validator_candidate(&validator_set, @0x2), 0);
        assert!(validator_set::is_inactive_validator(&validator_set, pool_id_2), 0);

        test_utils::destroy(validator_set);
        test_scenario::end(scenario_val);
    }

    fun create_validator(addr: address, hint: u8, gas_price: u64, is_initial_validator: bool, ctx: &mut TxContext): Validator {
        let stake_value = (hint as u64) * 100;
        let name = hint_to_ascii(hint);
        let validator = validator::new_for_testing(
            addr,
            vector[hint],
            vector[hint],
            vector[hint],
            vector[hint],
            copy name,
            copy name,
            copy name,
            name,
            vector[hint],
            vector[hint],
            vector[hint],
            vector[hint],
            option::some(balance::create_for_testing(stake_value)),
            gas_price,
            0,
            is_initial_validator,
            ctx
        );
        validator
    }

    fun hint_to_ascii(hint: u8): vector<u8> {
        let ascii_bytes = vector[hint / 100 + 65, hint % 100 / 10 + 65, hint % 10 + 65];
        ascii::into_bytes(ascii::string(ascii_bytes))
    }

    fun advance_epoch_with_dummy_rewards(validator_set: &mut ValidatorSet, scenario: &mut Scenario) {
        test_scenario::next_epoch(scenario, @0x0);
        let dummy_computation_reward = balance::zero();
        let dummy_storage_fund_reward = balance::zero();

        validator_set::advance_epoch(
            validator_set,
            &mut dummy_computation_reward,
            &mut dummy_storage_fund_reward,
            &mut vec_map::empty(),
            0,
            test_scenario::ctx(scenario)
        );

        balance::destroy_zero(dummy_computation_reward);
        balance::destroy_zero(dummy_storage_fund_reward);
    }

    fun add_and_activate_validator(validator_set: &mut ValidatorSet, validator: Validator, scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, validator::sui_address(&validator));
        validator_set::request_add_validator_candidate(validator_set, validator);
        validator_set::request_add_validator(validator_set, 0, test_scenario::ctx(scenario));
    }
}
