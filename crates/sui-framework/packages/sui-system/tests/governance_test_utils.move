// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::governance_test_utils {
    use sui::address;
    use sui::balance;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui_system::staking_pool::{StakedSui, StakingPool};
    use sui::test_utils::assert_eq;
    use sui_system::validator::{Self, Validator};
    use sui_system::sui_system::{Self, SuiSystemState};
    use sui_system::sui_system_state_inner;
    use sui_system::stake_subsidy;
    use sui::test_scenario::{Self, Scenario};
    use sui::test_utils;
    use sui::balance::Balance;

    const MIST_PER_SUI: u64 = 1_000_000_000;

    public fun create_validator_for_testing(
        addr: address, init_stake_amount_in_sui: u64, ctx: &mut TxContext
    ): Validator {
        let validator = validator::new_for_testing(
            addr,
            x"AA",
            x"BB",
            x"CC",
            x"DD",
            b"ValidatorName",
            b"description",
            b"image_url",
            b"project_url",
            b"/ip4/127.0.0.1/tcp/80",
            b"/ip4/127.0.0.1/udp/80",
            b"/ip4/127.0.0.1/udp/80",
            b"/ip4/127.0.0.1/udp/80",
            option::some(balance::create_for_testing<SUI>(init_stake_amount_in_sui * MIST_PER_SUI)),
            1,
            0,
            true,
            ctx
        );
        validator
    }

    /// Create a validator set with the given stake amounts
    public fun create_validators_with_stakes(stakes: vector<u64>, ctx: &mut TxContext): vector<Validator> {
        let mut i = 0;
        let mut validators = vector[];
        while (i < stakes.length()) {
            let validator = create_validator_for_testing(address::from_u256(i as u256), stakes[i], ctx);
            validators.push_back(validator);
            i = i + 1
        };
        validators
    }

    public fun create_sui_system_state_for_testing(
        validators: vector<Validator>, sui_supply_amount: u64, storage_fund_amount: u64, ctx: &mut TxContext
    ) {
        let system_parameters = sui_system_state_inner::create_system_parameters(
            42,  // epoch_duration_ms, doesn't matter what number we put here
            0,   // stake_subsidy_start_epoch

            150, // max_validator_count
            1,   // min_validator_joining_stake
            1,   // validator_low_stake_threshold
            0,   // validator_very_low_stake_threshold
            7,   // validator_low_stake_grace_period
            ctx,
        );

        let stake_subsidy = stake_subsidy::create(
            balance::create_for_testing<SUI>(sui_supply_amount * MIST_PER_SUI), // sui_supply
            0,   // stake subsidy initial distribution amount
            10,  // stake_subsidy_period_length
            0,   // stake_subsidy_decrease_rate
            ctx,
        );

        sui_system::create(
            object::new(ctx), // it doesn't matter what ID sui system state has in tests
            validators,
            balance::create_for_testing<SUI>(storage_fund_amount * MIST_PER_SUI), // storage_fund
            1,   // protocol version
            0,   // chain_start_timestamp_ms
            system_parameters,
            stake_subsidy,
            ctx,
        )
    }

    public fun set_up_sui_system_state(mut addrs: vector<address>) {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let mut validators = vector[];

        while (!addrs.is_empty()) {
            validators.push_back(
                create_validator_for_testing(addrs.pop_back(), 100, ctx)
            );
        };

        create_sui_system_state_for_testing(validators, 1000, 0, ctx);
        scenario.end();
    }

    public fun advance_epoch(scenario: &mut Scenario) {
        advance_epoch_with_reward_amounts(0, 0, scenario);
    }

    public fun advance_epoch_with_reward_amounts_return_rebate(
        storage_charge: u64, computation_charge: u64, stoarge_rebate: u64, non_refundable_storage_rebate: u64, scenario: &mut Scenario,
    ): Balance<SUI> {
        scenario.next_tx(@0x0);
        let new_epoch = scenario.ctx().epoch() + 1;
        let mut system_state = scenario.take_shared<SuiSystemState>();

        let ctx = scenario.ctx();

        let storage_rebate = system_state.advance_epoch_for_testing(
            new_epoch, 1, storage_charge, computation_charge, stoarge_rebate, non_refundable_storage_rebate, 0, 0, 0, ctx,
        );
        test_scenario::return_shared(system_state);
        scenario.next_epoch(@0x0);
        storage_rebate
    }

    public fun advance_epoch_with_reward_amounts(
        storage_charge: u64, computation_charge: u64, scenario: &mut Scenario
    ) {
        let storage_rebate = advance_epoch_with_reward_amounts_return_rebate(storage_charge * MIST_PER_SUI, computation_charge * MIST_PER_SUI, 0, 0, scenario);
        test_utils::destroy(storage_rebate)
    }

    public fun advance_epoch_with_reward_amounts_and_slashing_rates(
        storage_charge: u64,
        computation_charge: u64,
        reward_slashing_rate: u64,
        scenario: &mut Scenario
    ) {
        scenario.next_tx(@0x0);
        let new_epoch = scenario.ctx().epoch() + 1;
        let mut system_state = scenario.take_shared<SuiSystemState>();

        let ctx = scenario.ctx();

        let storage_rebate = system_state.advance_epoch_for_testing(
            new_epoch, 1, storage_charge * MIST_PER_SUI, computation_charge * MIST_PER_SUI, 0, 0, 0, reward_slashing_rate, 0, ctx
        );
        test_utils::destroy(storage_rebate);
        test_scenario::return_shared(system_state);
        scenario.next_epoch(@0x0);
    }

    public fun stake_with(
        staker: address, validator: address, amount: u64, scenario: &mut Scenario
    ) {
        scenario.next_tx(staker);
        let mut system_state = scenario.take_shared<SuiSystemState>();

        let ctx = scenario.ctx();

        system_state.request_add_stake(coin::mint_for_testing(amount * MIST_PER_SUI, ctx), validator, ctx);
        test_scenario::return_shared(system_state);
    }

    public fun unstake(
        staker: address, staked_sui_idx: u64, scenario: &mut Scenario
    ) {
        scenario.next_tx(staker);
        let stake_sui_ids = scenario.ids_for_sender<StakedSui>();
        let staked_sui = scenario.take_from_sender_by_id(stake_sui_ids[staked_sui_idx]);
        let mut system_state = scenario.take_shared<SuiSystemState>();

        let ctx = scenario.ctx();
        system_state.request_withdraw_stake(staked_sui, ctx);
        test_scenario::return_shared(system_state);
    }

    public fun add_validator_full_flow(validator: address, name: vector<u8>, net_addr: vector<u8>, init_stake_amount: u64, pubkey: vector<u8>, pop: vector<u8>, scenario: &mut Scenario) {
        scenario.next_tx(validator);
        let mut system_state = scenario.take_shared<SuiSystemState>();
        let ctx = scenario.ctx();

        system_state.request_add_validator_candidate(
            pubkey,
            vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
            vector[171, 3, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
            pop,
            name,
            b"description",
            b"image_url",
            b"project_url",
            net_addr,
            net_addr,
            net_addr,
            net_addr,
            1,
            0,
            ctx
        );
        system_state.request_add_stake(coin::mint_for_testing<SUI>(init_stake_amount * MIST_PER_SUI, ctx), validator, ctx);
        system_state.request_add_validator_for_testing(0, ctx);
        test_scenario::return_shared(system_state);
    }

    public fun add_validator_candidate(validator: address, name: vector<u8>, net_addr: vector<u8>, pubkey: vector<u8>, pop: vector<u8>, scenario: &mut Scenario) {
        scenario.next_tx(validator);
        let mut system_state = scenario.take_shared<SuiSystemState>();
        let ctx = scenario.ctx();

        system_state.request_add_validator_candidate(
            pubkey,
            vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
            vector[171, 3, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
            pop,
            name,
            b"description",
            b"image_url",
            b"project_url",
            net_addr,
            net_addr,
            net_addr,
            net_addr,
            1,
            0,
            ctx
        );
        test_scenario::return_shared(system_state);
    }

    public fun remove_validator_candidate(validator: address, scenario: &mut Scenario) {
        scenario.next_tx(validator);
        let mut system_state = scenario.take_shared<SuiSystemState>();
        let ctx = scenario.ctx();

        system_state.request_remove_validator_candidate(ctx);
        test_scenario::return_shared(system_state);
    }

    public fun add_validator(validator: address, scenario: &mut Scenario) {
        scenario.next_tx(validator);
        let mut system_state = scenario.take_shared<SuiSystemState>();
        let ctx = scenario.ctx();

        system_state.request_add_validator_for_testing(0, ctx);
        test_scenario::return_shared(system_state);
    }

    public fun remove_validator(validator: address, scenario: &mut Scenario) {
        scenario.next_tx(validator);
        let mut system_state = scenario.take_shared<SuiSystemState>();

        let ctx = scenario.ctx();

        system_state.request_remove_validator(ctx);
        test_scenario::return_shared(system_state);
    }

    public fun assert_validator_self_stake_amounts(validator_addrs: vector<address>, stake_amounts: vector<u64>, scenario: &mut Scenario) {
        let mut i = 0;
        while (i < validator_addrs.length()) {
            let validator_addr = validator_addrs[i];
            let amount = stake_amounts[i];

            scenario.next_tx(validator_addr);
            let mut system_state = scenario.take_shared<SuiSystemState>();
            let stake_plus_rewards = stake_plus_current_rewards_for_validator(validator_addr, &mut system_state, scenario);
            assert_eq(stake_plus_rewards, amount);
            test_scenario::return_shared(system_state);
            i = i + 1;
        };
    }

    public fun assert_validator_total_stake_amounts(validator_addrs: vector<address>, stake_amounts: vector<u64>, scenario: &mut Scenario) {
        let mut i = 0;
        while (i < validator_addrs.length()) {
            let validator_addr = validator_addrs[i];
            let amount = stake_amounts[i];

            scenario.next_tx(validator_addr);
            let mut system_state = scenario.take_shared<SuiSystemState>();
            let validator_amount = system_state.validator_stake_amount(validator_addr);
            assert!(validator_amount == amount, validator_amount);
            test_scenario::return_shared(system_state);
            i = i + 1;
        };
    }

    public fun assert_validator_non_self_stake_amounts(validator_addrs: vector<address>, stake_amounts: vector<u64>, scenario: &mut Scenario) {
        let mut i = 0;
        while (i < validator_addrs.length()) {
            let validator_addr = validator_addrs[i];
            let amount = stake_amounts[i];
            scenario.next_tx(validator_addr);
            let mut system_state = scenario.take_shared<SuiSystemState>();
            let non_self_stake_amount = system_state.validator_stake_amount(validator_addr) - stake_plus_current_rewards_for_validator(validator_addr, &mut system_state, scenario);
            assert_eq(non_self_stake_amount, amount);
            test_scenario::return_shared(system_state);
            i = i + 1;
        };
    }

    /// Return the rewards for the validator at `addr` in terms of SUI.
    public fun stake_plus_current_rewards_for_validator(addr: address, system_state: &mut SuiSystemState, scenario: &mut Scenario): u64 {
        let validator_ref = system_state.validators().get_active_validator_ref(addr);
        let amount = stake_plus_current_rewards(addr, validator_ref.get_staking_pool_ref(), scenario);
        amount
    }

    public fun stake_plus_current_rewards(addr: address, staking_pool: &StakingPool, scenario: &mut Scenario): u64 {
        let mut sum = 0;
        scenario.next_tx(addr);
        let mut stake_ids = scenario.ids_for_sender<StakedSui>();
        let current_epoch = scenario.ctx().epoch();

        while (!stake_ids.is_empty()) {
            let staked_sui_id = stake_ids.pop_back();
            let staked_sui = scenario.take_from_sender_by_id<StakedSui>(staked_sui_id);
            sum = sum + staking_pool.calculate_rewards(&staked_sui, current_epoch);
            scenario.return_to_sender(staked_sui);
        };
        sum
    }

    public fun total_sui_balance(addr: address, scenario: &mut Scenario): u64 {
        let mut sum = 0;
        scenario.next_tx(addr);
        let coin_ids = scenario.ids_for_sender<Coin<SUI>>();
        let mut i = 0;
        while (i < coin_ids.length()) {
            let coin = scenario.take_from_sender_by_id<Coin<SUI>>(coin_ids[i]);
            sum = sum + coin.value();
            scenario.return_to_sender(coin);
            i = i + 1;
        };
        sum
    }
}
