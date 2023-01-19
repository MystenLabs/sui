// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::governance_test_utils {
    use sui::address;
    use sui::balance;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::stake::{Self, Stake};
    use sui::staking_pool::{StakedSui, Delegation};
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::test_scenario::{Self, Scenario};
    use std::option;
    use std::vector;

    public fun create_validator_for_testing(
        addr: address, init_stake_amount: u64, ctx: &mut TxContext
    ): Validator {
        validator::new_for_testing(
            addr,
            x"FF",
            x"FF",
            x"FF",
            x"FF",
            b"ValidatorName",
            b"description",
            b"image_url",
            b"project_url",
            x"FFFF",
            x"FFFF",
            x"FFFF",
            balance::create_for_testing<SUI>(init_stake_amount),
            option::none(),
            1,
            0,
            ctx
        )
    }

    /// Create a validator set with the given stake amounts
    public fun create_validators_with_stakes(stakes: vector<u64>, ctx: &mut TxContext): vector<Validator> {
        let i = 0;
        let validators = vector[];
        while (i < vector::length(&stakes)) {
            let validator = create_validator_for_testing(address::from_u256((i as u256)), *vector::borrow(&stakes, i), ctx);
            vector::push_back(&mut validators, validator);
            i = i + 1
        };
        validators
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
            1, // storage_gas_price
            0, // stake subsidy
        )
    }

    public fun set_up_sui_system_state(addrs: vector<address>, scenario: &mut Scenario) {
        let ctx = test_scenario::ctx(scenario);
        let validators = vector::empty();

        while (!vector::is_empty(&addrs)) {
            vector::push_back(
                &mut validators,
                create_validator_for_testing(vector::pop_back(&mut addrs), 100, ctx)
            );
        };

        create_sui_system_state_for_testing(validators, 1000, 0);
    }

    public fun advance_epoch(scenario: &mut Scenario) {
        advance_epoch_with_reward_amounts(0, 0, scenario);
    }

    public fun advance_epoch_with_reward_amounts(
        storage_charge: u64, computation_charge: u64, scenario: &mut Scenario
    ) {
        test_scenario::next_epoch(scenario, @0x0);
        let new_epoch = tx_context::epoch(test_scenario::ctx(scenario));
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

        let ctx = test_scenario::ctx(scenario);

        sui_system::advance_epoch(&mut system_state, new_epoch, storage_charge, computation_charge, 0, 0, 0, 0, ctx);
        test_scenario::return_shared(system_state);
    }

    public fun advance_epoch_with_reward_amounts_and_slashing_rates(
        storage_charge: u64,
        computation_charge: u64,
        reward_slashing_threshold_bps: u64,
        reward_slashing_rate: u64,
        scenario: &mut Scenario
    ) {
        test_scenario::next_epoch(scenario, @0x0);
        let new_epoch = tx_context::epoch(test_scenario::ctx(scenario));
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

        let ctx = test_scenario::ctx(scenario);

        sui_system::advance_epoch(
            &mut system_state, new_epoch, storage_charge, computation_charge, 0, 0,
            reward_slashing_threshold_bps, reward_slashing_rate, ctx
        );
        test_scenario::return_shared(system_state);
    }

    public fun delegate_to(
        delegator: address, validator: address, amount: u64, scenario: &mut Scenario
    ) {
        test_scenario::next_tx(scenario, delegator);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

        let ctx = test_scenario::ctx(scenario);

        sui_system::request_add_delegation(&mut system_state, coin::mint_for_testing(amount, ctx), validator, ctx);
        test_scenario::return_shared(system_state);
    }

    public fun undelegate(
        delegator: address, staked_sui_idx: u64, delegation_obj_idx: u64, scenario: &mut Scenario
    ) {
        test_scenario::next_tx(scenario, delegator);
        let stake_sui_ids = test_scenario::ids_for_sender<StakedSui>(scenario);
        let staked_sui = test_scenario::take_from_sender_by_id(scenario, *vector::borrow(&stake_sui_ids, staked_sui_idx));
        let delegation_ids = test_scenario::ids_for_sender<Delegation>(scenario);
        let delegation = test_scenario::take_from_sender_by_id(scenario, *vector::borrow(&delegation_ids, delegation_obj_idx));
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

        let ctx = test_scenario::ctx(scenario);
        sui_system::request_withdraw_delegation(&mut system_state, delegation, staked_sui, ctx);
        test_scenario::return_shared(system_state);
    }

    public fun assert_validator_stake_amounts(validator_addrs: vector<address>, stake_amounts: vector<u64>, scenario: &mut Scenario) {
        let i = 0;
        while (i < vector::length(&validator_addrs)) {
            let validator_addr = *vector::borrow(&validator_addrs, i);
            let amount = *vector::borrow(&stake_amounts, i);
            assert!(sum_up_validator_stake_amounts(validator_addr, scenario) == amount, 0);

            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            assert!(sui_system::validator_stake_amount(&mut system_state, validator_addr) == amount, 0);
            test_scenario::return_shared(system_state);
            i = i + 1;
        };
    }

    public fun assert_validator_delegate_amounts(validator_addrs: vector<address>, delegate_amounts: vector<u64>, scenario: &mut Scenario) {
        let i = 0;
        while (i < vector::length(&validator_addrs)) {
            let validator_addr = *vector::borrow(&validator_addrs, i);
            let amount = *vector::borrow(&delegate_amounts, i);
            test_scenario::next_tx(scenario, validator_addr);
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            assert!(sui_system::validator_delegate_amount(&mut system_state, validator_addr) == amount, 0);
            test_scenario::return_shared(system_state);
            i = i + 1;
        };
    }

    public fun sum_up_validator_stake_amounts(addr: address, scenario: &mut Scenario): u64 {
        let sum = 0;
        test_scenario::next_tx(scenario, addr);
        let stake_ids = test_scenario::ids_for_sender<Stake>(scenario);
        let i = 0;
        while (i < vector::length(&stake_ids)) {
            let stake = test_scenario::take_from_sender_by_id(scenario, *vector::borrow(&stake_ids, i));
            sum = sum + stake::value(&stake);
            test_scenario::return_to_sender(scenario, stake);
            i = i + 1;
        };
        sum
    }

    public fun total_sui_balance(addr: address, scenario: &mut Scenario): u64 {
        let sum = 0;
        test_scenario::next_tx(scenario, addr);
        let coin_ids = test_scenario::ids_for_sender<Coin<SUI>>(scenario);
        let i = 0;
        while (i < vector::length(&coin_ids)) {
            let coin = test_scenario::take_from_sender_by_id<Coin<SUI>>(scenario, *vector::borrow(&coin_ids, i));
            sum = sum + coin::value(&coin);
            test_scenario::return_to_sender(scenario, coin);
            i = i + 1;
        };
        sum
    }
}
