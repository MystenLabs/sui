// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::validator_set_tests {
    use sui::balance;
    use sui::coin;
    use sui_system::staking_pool::StakedSui;
    use sui_system::validator::{Self, Validator, staking_pool_id};
    use sui_system::validator_set::{Self, ValidatorSet, active_validator_addresses};
    use sui::test_scenario::{Self, Scenario};
    use sui::test_utils::{Self, assert_eq};
    use sui::vec_map;

    const MIST_PER_SUI: u64 = 1_000_000_000; // used internally for stakes.

    #[test]
    fun test_validator_set_flow() {
        // Create 4 validators, with stake 100, 200, 300, 400. Only the first validator is an initial validator.
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        let validator1 = create_validator(@0x1, 1, 1, true, ctx);
        let validator2 = create_validator(@0x2, 2, 1, false, ctx);
        let validator3 = create_validator(@0x3, 3, 1, false, ctx);
        let validator4 = create_validator(@0x4, 4, 1, false, ctx);

        // Create a validator set with only the first validator in it.
        let mut validator_set = validator_set::new(vector[validator1], ctx);
        assert!(validator_set.total_stake() == 100 * MIST_PER_SUI);

        // Add the other 3 validators one by one.
        add_and_activate_validator(
            &mut validator_set,
            validator2,
            scenario
        );
        // Adding validator during the epoch should not affect stake and quorum threshold.
        assert!(validator_set.total_stake() == 100 * MIST_PER_SUI);

        add_and_activate_validator(
            &mut validator_set,
            validator3,
            scenario
        );
        scenario_val.end();

        let mut scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        {
            let ctx1 = scenario.ctx();
            let stake = validator_set.request_add_stake(
                @0x1,
                coin::mint_for_testing(500 * MIST_PER_SUI, ctx1).into_balance(),
                ctx1,
            );
            transfer::public_transfer(stake, @0x1);
            // Adding stake to existing active validator during the epoch
            // should not change total stake.
            assert!(validator_set.total_stake() == 100 * MIST_PER_SUI);
        };

        add_and_activate_validator(
            &mut validator_set,
            validator4,
            scenario
        );

        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);
        // Total stake for these should be the starting stake + the 500 staked with validator 1 in addition to the starting stake.
        assert!(validator_set.total_stake() == 1500 * MIST_PER_SUI);

        scenario.next_tx(@0x1);
        {
            let ctx1 = scenario.ctx();

            validator_set.request_remove_validator(ctx1);
        };

        // Total validator candidate count changes, but total stake remains during epoch.
        assert!(validator_set.total_stake() == 1500 * MIST_PER_SUI);
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);
        // Validator1 is gone. This removes its stake (100) + the 500 staked with it.
        assert!(validator_set.total_stake() == 900 * MIST_PER_SUI);

        test_utils::destroy(validator_set);
        scenario_val.end();
    }

    #[test]
    fun test_reference_gas_price_derivation() {
        // Create 5 validators with different stakes and different gas prices.
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        let v1 = create_validator(@0x1, 1, 45, true, ctx);
        let v2 = create_validator(@0x2, 2, 42, false, ctx);
        let v3 = create_validator(@0x3, 3, 40, false, ctx);
        let v4 = create_validator(@0x4, 4, 41, false, ctx);
        let v5 = create_validator(@0x5, 10, 43, false, ctx);
        // Create a validator set with only the first validator in it.
        let mut validator_set = validator_set::new(vector[v1], ctx);

        assert_eq(validator_set.derive_reference_gas_price(), 45);

        add_and_activate_validator(&mut validator_set, v2, scenario);
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);

        assert_eq(validator_set.derive_reference_gas_price(), 45);

        add_and_activate_validator(
            &mut validator_set,
            v3,
            scenario
        );
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);

        assert_eq(validator_set.derive_reference_gas_price(), 42);

        add_and_activate_validator(
            &mut validator_set,
            v4,
            scenario
        );
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);

        assert_eq(validator_set.derive_reference_gas_price(), 42);

        add_and_activate_validator(
            &mut validator_set,
            v5,
            scenario
        );
        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);

        assert_eq(validator_set.derive_reference_gas_price(), 43);

        test_utils::destroy(validator_set);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator_set::EStakingBelowThreshold)]
    fun test_staking_below_threshold() {
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();

        let validator1 = create_validator(@0x1, 1, 1, true, ctx);
        let mut validator_set = validator_set::new(vector[validator1], ctx);
        assert_eq(validator_set.total_stake(), 100 * MIST_PER_SUI);
        scenario_val.end();

        let mut scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = scenario.ctx();

        let stake = validator_set.request_add_stake(
            @0x1,
            balance::create_for_testing(MIST_PER_SUI - 1), // 1 MIST lower than the threshold
            ctx1,
        );
        transfer::public_transfer(stake, @0x1);
        test_utils::destroy(validator_set);
        scenario_val.end();
    }

    #[test]
    fun test_staking_min_threshold() {
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();

        let validator1 = create_validator(@0x1, 1, 1, true, ctx);
        let mut validator_set = validator_set::new(vector[validator1], ctx);
        assert_eq(validator_set.total_stake(), 100 * MIST_PER_SUI);
        scenario_val.end();

        let mut scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = scenario.ctx();
        let stake = validator_set.request_add_stake(
            @0x1,
            balance::create_for_testing(MIST_PER_SUI), // min possible stake
            ctx1,
        );
        transfer::public_transfer(stake, @0x1);

        advance_epoch_with_dummy_rewards(&mut validator_set, scenario);
        assert!(validator_set.total_stake() == 101 * MIST_PER_SUI);

        test_utils::destroy(validator_set);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator_set::EMinJoiningStakeNotReached)]
    fun test_add_validator_failure_below_min_stake() {
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();

        // Create 2 validators, with stake 100 and 200.
        let validator1 = create_validator(@0x1, 1, 1, true, ctx);
        let validator2 = create_validator(@0x2, 2, 1, false, ctx);

        // Create a validator set with only the first validator in it.
        let mut validator_set = validator_set::new(vector[validator1], ctx);
        assert_eq(validator_set.total_stake(), 100 * MIST_PER_SUI);
        scenario_val.end();

        let mut scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = scenario.ctx();
        validator_set.request_add_validator_candidate(validator2, ctx1);

        scenario.next_tx(@0x42);
        {
            let ctx = scenario.ctx();
            let stake = validator_set.request_add_stake(
                @0x2,
                balance::create_for_testing(500 * MIST_PER_SUI),
                ctx,
            );
            transfer::public_transfer(stake, @0x42);
            // Adding stake to a preactive validator should not change total stake.
            assert_eq(validator_set.total_stake(), 100 * MIST_PER_SUI);
        };

        scenario.next_tx(@0x2);
        // Validator 2 now has 700 SUI in stake but that's not enough because we need 701.
        validator_set.request_add_validator(701 * MIST_PER_SUI, scenario.ctx());

        test_utils::destroy(validator_set);
        scenario_val.end();
    }

    #[test]
    fun test_add_validator_with_nonzero_min_stake() {
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();

        // Create 2 validators, with stake 100 and 200.
        let validator1 = create_validator(@0x1, 1, 1, true, ctx);
        let validator2 = create_validator(@0x2, 2, 1, false, ctx);

        // Create a validator set with only the first validator in it.
        let mut validator_set = validator_set::new(vector[validator1], ctx);
        assert_eq(validator_set.total_stake(), 100 * MIST_PER_SUI);
        scenario_val.end();

        let mut scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = scenario.ctx();
        validator_set.request_add_validator_candidate(validator2, ctx1);

        scenario.next_tx(@0x42);
        {
            let ctx = scenario.ctx();
            let stake = validator_set.request_add_stake(
                @0x2,
                balance::create_for_testing(500 * MIST_PER_SUI),
                ctx,
            );
            transfer::public_transfer(stake, @0x42);
            // Adding stake to a preactive validator should not change total stake.
            assert_eq(validator_set.total_stake(), 100 * MIST_PER_SUI);
        };

        scenario.next_tx(@0x2);
        // Validator 2 now has 700 SUI in stake and that's just enough.
        validator_set.request_add_validator(700 * MIST_PER_SUI, scenario.ctx());

        test_utils::destroy(validator_set);
        scenario_val.end();
    }

    #[test]
    fun test_add_candidate_then_remove() {
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();

        // Create 2 validators, with stake 100 and 200.
        let validator1 = create_validator(@0x1, 1, 1, true, ctx);
        let validator2 = create_validator(@0x2, 2, 1, false, ctx);

        let pool_id_2 = staking_pool_id(&validator2);

        // Create a validator set with only the first validator in it.
        let mut validator_set = validator_set::new(vector[validator1], ctx);
        assert_eq(validator_set.total_stake(), 100 * MIST_PER_SUI);
        scenario_val.end();

        let mut scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        let ctx1 = scenario.ctx();
        // Add the second one as a candidate.
        validator_set.request_add_validator_candidate(validator2, ctx1);
        assert!(validator_set.is_validator_candidate(@0x2));
        assert_eq(validator_set.validator_address_by_pool_id(&pool_id_2), @0x2);

        scenario.next_tx(@0x2);
        // Then remove its candidacy.
        validator_set.request_remove_validator_candidate(scenario.ctx());
        assert!(!validator_set.is_validator_candidate(@0x2));
        assert!(validator_set.is_inactive_validator(pool_id_2));
        assert_eq(validator_set.validator_address_by_pool_id(&pool_id_2), @0x2);

        test_utils::destroy(validator_set);
        scenario_val.end();
    }

    #[test]
    fun test_low_stake_departure() {
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        // Create 4 validators.
        let v1 = create_validator(@0x1, 1, 1, true, ctx); // 100 SUI of stake
        let v2 = create_validator(@0x2, 4, 1, true, ctx); // 400 SUI of stake
        let v3 = create_validator(@0x3, 10, 1, true, ctx); // 1000 SUI of stake
        let v4 = create_validator(@0x4, 4, 1, true, ctx); // 400 SUI of stake

        let mut validator_set = validator_set::new(vector[v1, v2, v3, v4], ctx);
        scenario_val.end();

        let mut scenario_val = test_scenario::begin(@0x1);
        let scenario = &mut scenario_val;
        assert_eq(active_validator_addresses(&validator_set), vector[@0x1, @0x2, @0x3, @0x4]);

        advance_epoch_with_low_stake_params(
            &mut validator_set, 500, 200, 3, scenario
        );

        // v1 is kicked out because their stake 100 is less than the very low stake threshold
        // which is 200.
        assert_eq(active_validator_addresses(&validator_set), vector[@0x2, @0x3, @0x4]);

        advance_epoch_with_low_stake_params(
            &mut validator_set, 500, 200, 3, scenario
        );
        assert_eq(active_validator_addresses(&validator_set), vector[@0x2, @0x3, @0x4]);

        advance_epoch_with_low_stake_params(
            &mut validator_set, 500, 200, 3, scenario
        );
        assert_eq(active_validator_addresses(&validator_set), vector[@0x2, @0x3, @0x4]);

        // Add some stake to @0x4 to get her out of the danger zone.
        scenario.next_tx(@0x42);
        {
            let ctx = scenario.ctx();
            let stake = validator_set.request_add_stake(
                @0x4,
                balance::create_for_testing(500 * MIST_PER_SUI),
                ctx,
            );
            transfer::public_transfer(stake, @0x42);
        };

        // So only @0x2 will be kicked out.
        advance_epoch_with_low_stake_params(
            &mut validator_set, 500, 200, 3, scenario
        );
        assert_eq(active_validator_addresses(&validator_set), vector[@0x3, @0x4]);

        // Withdraw the stake from @0x4.
        scenario.next_tx(@0x42);
        {
            let stake = scenario.take_from_sender<StakedSui>();
            let ctx = scenario.ctx();
            let withdrawn_balance = validator_set.request_withdraw_stake(
                stake,
                ctx,
            );
            transfer::public_transfer(withdrawn_balance.into_coin(ctx), @0x42);
        };

        // Now @0x4 gets kicked out after 3 grace days are used at the 4th epoch change.
        advance_epoch_with_low_stake_params(
            &mut validator_set, 500, 200, 3, scenario
        );
        assert_eq(active_validator_addresses(&validator_set), vector[@0x3, @0x4]);
        advance_epoch_with_low_stake_params(
            &mut validator_set, 500, 200, 3, scenario
        );
        assert_eq(active_validator_addresses(&validator_set), vector[@0x3, @0x4]);
        advance_epoch_with_low_stake_params(
            &mut validator_set, 500, 200, 3, scenario
        );
        assert_eq(active_validator_addresses(&validator_set), vector[@0x3, @0x4]);
        advance_epoch_with_low_stake_params(
            &mut validator_set, 500, 200, 3, scenario
        );
        // @0x4 was kicked out.
        assert_eq(active_validator_addresses(&validator_set), vector[@0x3]);
        test_utils::destroy(validator_set);
        scenario_val.end();
    }

    fun create_validator(addr: address, hint: u8, gas_price: u64, is_initial_validator: bool, ctx: &mut TxContext): Validator {
        let stake_value = hint as u64 * 100 * MIST_PER_SUI;
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
        ascii_bytes.to_ascii_string().into_bytes()
    }

    fun advance_epoch_with_dummy_rewards(validator_set: &mut ValidatorSet, scenario: &mut Scenario) {
        scenario.next_epoch(@0x0);
        let mut dummy_computation_reward = balance::zero();
        let mut dummy_storage_fund_reward = balance::zero();

        validator_set.advance_epoch(
            &mut dummy_computation_reward,
            &mut dummy_storage_fund_reward,
            &mut vec_map::empty(),
            0, // reward_slashing_rate
            0, // low_stake_threshold
            0, // very_low_stake_threshold
            0, // low_stake_grace_period
            scenario.ctx()
        );

        dummy_computation_reward.destroy_zero();
        dummy_storage_fund_reward.destroy_zero();
    }

    fun advance_epoch_with_low_stake_params(
        validator_set: &mut ValidatorSet,
        low_stake_threshold: u64,
        very_low_stake_threshold: u64,
        low_stake_grace_period: u64,
        scenario: &mut Scenario
    ) {
        scenario.next_epoch(@0x0);
        let mut dummy_computation_reward = balance::zero();
        let mut dummy_storage_fund_reward = balance::zero();
        validator_set.advance_epoch(
            &mut dummy_computation_reward,
            &mut dummy_storage_fund_reward,
            &mut vec_map::empty(),
            0, // reward_slashing_rate
            low_stake_threshold * MIST_PER_SUI,
            very_low_stake_threshold * MIST_PER_SUI,
            low_stake_grace_period,
            scenario.ctx()
        );

        dummy_computation_reward.destroy_zero();
        dummy_storage_fund_reward.destroy_zero();
    }

    fun add_and_activate_validator(validator_set: &mut ValidatorSet, validator: Validator, scenario: &mut Scenario) {
        scenario.next_tx(validator.sui_address());
        let ctx = scenario.ctx();
        validator_set.request_add_validator_candidate(validator, ctx);
        validator_set.request_add_validator(0, ctx);
    }
}
