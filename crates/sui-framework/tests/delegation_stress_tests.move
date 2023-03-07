// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::delegation_stress_tests {
    use sui::test_random::{
        Self, Random, next_u8, next_u64_in_range, next_address
    };
    use sui::coin::{Coin, mint_for_testing};
    use sui::vec_set::{Self, VecSet};
    use sui::sui::SUI;
    use sui::address;
    use sui::test_scenario::{
        Self, Scenario, is_owned_by_address_of_type,
        take_from_sender_by_id,
        most_recent_id_for_address
    };
    use sui::vec_map::{Self, VecMap};
    use std::vector;
    use std::option;
    use sui::object::ID;
    use sui::sui_system::{Self, SuiSystemState, storage_fund_balance};
    use sui::tx_context;
    use sui::governance_test_utils::{
        create_validators_with_stakes, create_sui_system_state_for_testing
    };
    use sui::staking_pool::StakedSui;

    struct TestState {
        scenario: Scenario,
        validators: vector<address>,
        delegation_requests_this_epoch: VecMap<ID, address>,
        delegation_withdraws_this_epoch: u64,
        cancelled_requests: VecSet<ID>,
        delegations: VecMap<ID, address>, // `StakedSui` objects and their owners that are active.
        random: Random,
    }

    struct StakedSuiInfo has drop {
        staked_sui_id: ID,
        delegator: address
    }

    const MAX_COMPUTATION_REWARD_AMOUNT_PER_EPOCH: u64 = 1_000_000_000;
    const MAX_STORAGE_REWARD_AMOUNT_PER_EPOCH: u64 = 1_000_000_000;

    const MAX_NUM_OPERATIONS_PER_EPOCH: u64 = 100;

    const MAX_DELEGATION_AMOUNT: u64 = 2000;
    const MAX_INIT_VALIDATOR_STAKE: u64 = 100_000;
    const MAX_INIT_STORAGE_FUND: u64 = 1_000_000;
    const MAX_GAS_PRICE: u64 = 1000;
    const NUM_VALIDATORS: u64 = 40;
    const BASIS_POINT_DENOMINATOR: u64 = 10_000;

    const NUM_OPERATIONS: u8 = 4;
    const ADD_DELEGATION: u8 = 0;
    const WITHDRAW_DELEGATION: u8 = 1;
    const SET_GAS_PRICE: u8 = 2;
    const REPORT_VALIDATOR: u8 = 3;

    #[test]
    fun smoke_test() {
        let state = begin(vector[42], 4);
        let num_epochs = 5;
        let i = 0;
        while (i < num_epochs) {
            i = i + 1;
            let num_operations_this_epoch = next_u64_in_range(&mut state.random, 10);
            let successful_operation_count = 0;
            while (successful_operation_count < num_operations_this_epoch) {
                if (possibly_perform_one_operation(&mut state)) {
                    successful_operation_count = successful_operation_count + 1;
                }
            };
            advance_epoch_with_random_rewards(&mut state);
        };
        end(state);
    }

    // Takes too long for CI
    //#[test]
    fun stress_test() {
        let state = begin(vector[42], NUM_VALIDATORS);
        let num_epochs = 50;
        let i = 0;
        while (i < num_epochs) {
            i = i + 1;
            let num_operations_this_epoch = next_u64_in_range(&mut state.random, MAX_NUM_OPERATIONS_PER_EPOCH);
            let successful_operation_count = 0;
            while (successful_operation_count < num_operations_this_epoch) {
                if (possibly_perform_one_operation(&mut state)) {
                    successful_operation_count = successful_operation_count + 1;
                }
            };
            advance_epoch_with_random_rewards(&mut state);
        };
        end(state);
        // abort 42
    }

    // Pick one random delegation operation.
    fun possibly_perform_one_operation(state: &mut TestState): bool {
        let operation_num = next_u8(&mut state.random) % NUM_OPERATIONS;
        if (operation_num == ADD_DELEGATION) {
            request_add_delegation_random(state)
        } else if (operation_num == WITHDRAW_DELEGATION) {
            request_withdraw_delegation_random(state)
        } else if (operation_num == SET_GAS_PRICE) {
            request_set_gas_price_random(state)
        } else if (operation_num == REPORT_VALIDATOR) {
            report_validator_random(state)
        } else {
            false
        }
    }

    fun request_set_gas_price_random(state: &mut TestState): bool {
        let validator = random_validator(state);
        let scenario = &mut state.scenario;
        test_scenario::next_tx(scenario, validator);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let ctx = test_scenario::ctx(scenario);
        let gas_price = next_u64_in_range(&mut state.random, MAX_GAS_PRICE);
        sui_system::request_set_gas_price(
            &mut system_state, gas_price, ctx);
        test_scenario::return_shared(system_state);
        test_scenario::next_tx(scenario, validator);
        true
    }

    fun report_validator_random(state: &mut TestState): bool {
        let validator = random_validator(state);
        let other = random_validator_except(validator, state);
        let scenario = &mut state.scenario;
        test_scenario::next_tx(scenario, validator);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let ctx = test_scenario::ctx(scenario);
        sui_system::report_validator(
            &mut system_state, other, ctx);
        test_scenario::return_shared(system_state);
        test_scenario::next_tx(scenario, validator);
        true
    }

    /// Pick a random address, delegate a random amount to a random validator.
    fun request_add_delegation_random(state: &mut TestState): bool {
        let scenario = &mut state.scenario;

        let delegator = next_address(&mut state.random);
        // let delegator = @0x42;
        let delegate_amount = next_u64_in_range(&mut state.random, MAX_DELEGATION_AMOUNT) + 1;

        let validator_idx = next_u64_in_range(&mut state.random, vector::length(&state.validators));
        let validator_address = *vector::borrow(&state.validators, validator_idx);
        test_scenario::next_tx(scenario, delegator);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let ctx = test_scenario::ctx(scenario);
        sui_system::request_add_delegation(
            &mut system_state, mint_for_testing<SUI>(delegate_amount, ctx), validator_address, ctx);
        test_scenario::return_shared(system_state);
        test_scenario::next_tx(scenario, delegator);

        let staked_sui_id = option::destroy_some(most_recent_id_for_address<StakedSui>(delegator));
        // vector::push_back(&mut state.delegation_requests_this_epoch, StakedSuiInfo { delegator, staked_sui_id });
        vec_map::insert(&mut state.delegation_requests_this_epoch, staked_sui_id, delegator);
        true
    }

    /// Pick a random existing delegation and withdraw it.
    /// Return false if no delegation is left in the state.
    fun request_withdraw_delegation_random(state: &mut TestState): bool {
        if (vec_map::is_empty(&mut state.delegations)) {
            return false
        };
        let scenario = &mut state.scenario;
        let num_delegations = vec_map::size(&state.delegations);
        let idx = next_u64_in_range(&mut state.random, num_delegations);
        let (delegation_id, delegator) = vec_map::remove_entry_by_idx(&mut state.delegations, idx);

        test_scenario::next_tx(scenario, delegator);
        let staked_sui = take_from_sender_by_id<StakedSui>(scenario, delegation_id);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        sui_system::request_withdraw_delegation(
            &mut system_state, staked_sui, test_scenario::ctx(scenario));
        test_scenario::return_shared(system_state);
        state.delegation_withdraws_this_epoch = state.delegation_withdraws_this_epoch + 1;
        true
    }

    fun advance_epoch_with_random_rewards(state: &mut TestState) {
        let scenario = &mut state.scenario;
        let rand = &mut state.random;

        test_scenario::next_tx(scenario, @0x0);
        let new_epoch = tx_context::epoch(test_scenario::ctx(scenario));

        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let storage_charge = next_u64_in_range(rand, MAX_STORAGE_REWARD_AMOUNT_PER_EPOCH);
        let computation_charge = next_u64_in_range(rand, MAX_COMPUTATION_REWARD_AMOUNT_PER_EPOCH);
        let max_storage_rebate = storage_fund_balance(&system_state) + storage_charge;
        let storage_rebate = next_u64_in_range(rand, max_storage_rebate);
        // call advance epoch txn
        sui_system::advance_epoch(
            &mut system_state,
            new_epoch + 1,
            2,
            storage_charge,
            computation_charge,
            storage_rebate,
            random_rate_basis_point(rand),
            random_rate_basis_point(rand),
            1, // can't use a random rate for stake subsidy because a big rate will cause overflow very quickly
            0,
            test_scenario::ctx(scenario)
        );
        test_scenario::return_shared(system_state);
        let txn_effect = test_scenario::next_epoch(scenario, @0x0);
        let new_objects = test_scenario::transferred_to_account(&txn_effect);
        let num_new_delegation_objs = 0;
        let num_new_coin_objs = 0;

        // filter out the new Delegation objects created during epoch change
        // and add them to `state.delegations`
        while (!vec_map::is_empty(&new_objects)) {
            let (id, owner) = vec_map::pop(&mut new_objects);
            if (is_owned_by_address_of_type<StakedSui>(owner, id)) {
                vec_map::insert(&mut state.delegations, id, owner);
                num_new_delegation_objs = num_new_delegation_objs + 1;
            } else if (is_owned_by_address_of_type<Coin<SUI>>(owner, id)) {
                num_new_coin_objs = num_new_coin_objs + 1;
            }
        };
        // assert!(num_new_delegation_objs == vector::length(&state.delegation_requests_this_epoch), 0);
        // assert!(num_new_coin_objs == state.delegation_withdraws_this_epoch, 0);
        // empty delegation_requests_this_epoch and delegation_withdraws_this_epoch
        state.delegation_requests_this_epoch = vec_map::empty();
        state.delegation_withdraws_this_epoch = 0;
        state.cancelled_requests = vec_set::empty();
    }

    fun begin(seed: vector<u8>, num_validators: u64): TestState {
        let scenario = test_scenario::begin(@0x0);
        let validators = vector[];
        let validator_stakes = vector[];
        let random = test_random::new(seed);
        let supply_amount = 0;
        let i = 0;
        while (i < num_validators) {
            let stake = next_u64_in_range(&mut random, MAX_INIT_VALIDATOR_STAKE);
            vector::push_back(&mut validator_stakes, stake);
            vector::push_back(&mut validators, address::from_u256((i as u256)));
            supply_amount = supply_amount + stake;
            i = i + 1;
        };
        let init_storage_fund = next_u64_in_range(&mut random, MAX_INIT_STORAGE_FUND);
        supply_amount = supply_amount + init_storage_fund;
        let ctx = test_scenario::ctx(&mut scenario);
        create_sui_system_state_for_testing(
            create_validators_with_stakes(validator_stakes, ctx),
            supply_amount,
            init_storage_fund,
            ctx
        );
        TestState {
            scenario,
            validators,
            delegation_requests_this_epoch: vec_map::empty(),
            cancelled_requests: vec_set::empty(),
            delegation_withdraws_this_epoch: 0,
            delegations: vec_map::empty(),
            random,
        }
    }

    fun end(state: TestState) {
        let TestState {
            scenario,
            validators: _,
            delegation_requests_this_epoch: _,
            delegation_withdraws_this_epoch: _,
            cancelled_requests: _,
            delegations: _,
            random: _
        } = state;
        test_scenario::end(scenario);
    }

    fun random_rate_basis_point(rand: &mut Random): u64 {
        next_u64_in_range(rand, BASIS_POINT_DENOMINATOR)
    }

    fun random_validator_except(excluded_validator: address, state: &mut TestState): address {
        let num_validators = vector::length(&state.validators);
        assert!(num_validators >= 2, 0);
        let idx = next_u64_in_range(&mut state.random, num_validators - 1);
        let chosen_validator = *vector::borrow(&state.validators, idx);
        if (chosen_validator == excluded_validator) {
            *vector::borrow(&state.validators, num_validators - 1)
        } else {
            chosen_validator
        }
    }

    fun random_validator(state: &mut TestState): address {
        let num_validators = vector::length(&state.validators);
        assert!(num_validators >= 1, 0);
        let idx = next_u64_in_range(&mut state.random, num_validators);
        *vector::borrow(&state.validators, idx)
    }
}
