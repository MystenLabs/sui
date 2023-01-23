// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::delegation_stress_tests {
    use sui::test_random::{
        Self, Random, next_u8, next_u64_in_range, next_address
    };
    use sui::stake::{Self, Stake};
    use sui::coin::{Coin, mint_for_testing};
    use sui::vec_set::{Self, VecSet};
    // use sui::balance;
    use sui::sui::SUI;
    use sui::address;
    use sui::test_scenario::{
        Self, Scenario, end_transaction, is_owned_by_address_of_type,
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
        create_validators_with_stakes, create_sui_system_state_for_testing};
    use sui::staking_pool::{Delegation, StakedSui, 
        staked_sui_id, validator_address
    };

    struct TestState {
        scenario: Scenario,
        validators: vector<address>,
        validator_stake_ids: VecMap<ID, address>,
        delegation_requests_this_epoch: VecMap<ID, address>,
        delegation_withdraws_this_epoch: u64,
        cancelled_requests: VecSet<ID>,
        delegations: VecMap<ID, address>, // `Delegation` objects and their owners
        random: Random,
    }

    struct StakedSuiInfo has drop {
        staked_sui_id: ID,
        delegator: address
    }

    const MAX_COMPUTATION_REWARD_AMOUNT_PER_EPOCH: u64 = 1_000_000_000;
    const MAX_STORAGE_REWARD_AMOUNT_PER_EPOCH: u64 = 1_000_000_000;

    const MAX_NUM_OPERATIONS_PER_EPOCH: u64 = 1000;

    const MAX_DELEGATION_AMOUNT: u64 = 2000;
    const MAX_INIT_VALIDATOR_STAKE: u64 = 100_000;
    const MAX_INIT_STORAGE_FUND: u64 = 1_000_000;
    const MAX_GAS_PRICE: u64 = 1000;
    const NUM_VALIDATORS: u64 = 40;
    const BASIS_POINT_DENOMINATOR: u64 = 10_000;

    const NUM_OPERATIONS: u8 = 8;
    const ADD_DELEGATION: u8 = 0;
    const CANCEL_DELEGATION: u8 = 1;
    const WITHDRAW_DELEGATION: u8 = 2;
    const SWITCH_DELEGATION: u8 = 3;
    const ADD_STAKE: u8 = 4;
    const WITHDRAW_STAKE: u8 = 5;
    const SET_GAS_PRICE: u8 = 6;
    const REPORT_VALIDATOR: u8 = 7;

    #[test]
    fun test() {
        let state = begin(vector[42]);
        let num_epochs = 100;
        let i = 0;
        while (i < num_epochs) {
            i = i + 1;
            std::debug::print(&i);
            let num_operations_this_epoch = next_u64_in_range(&mut state.random, MAX_NUM_OPERATIONS_PER_EPOCH);
            let successful_operation_count = 0;
            while (successful_operation_count < num_operations_this_epoch) {
                if (possibly_perform_one_operation(&mut state)) {
                    successful_operation_count = successful_operation_count + 1;
                } else {
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
        // std::debug::print(&operation_num);
        if (operation_num == ADD_DELEGATION) {
            request_add_delegation_random(state)
        } else if (operation_num == CANCEL_DELEGATION) {
            cancel_delegation_request_random(state)
        } else if (operation_num == WITHDRAW_DELEGATION) {
            request_withdraw_delegation_random(state)
        } else if (operation_num == SWITCH_DELEGATION) {
            request_switch_delegation_random(state)
        } else if (operation_num == ADD_STAKE) {
            request_add_stake_random(state)
        } else if (operation_num == WITHDRAW_STAKE) {
            request_withdraw_stake_random(state)
        } else if (operation_num == SET_GAS_PRICE) {
            request_set_gas_price_random(state)
        } else if (operation_num == REPORT_VALIDATOR) {
            report_validator_random(state)
        } else {
            false
        }
    }

    fun request_add_stake_random(state: &mut TestState): bool {
        let validator = random_validator(state);
        let scenario = &mut state.scenario;
        test_scenario::next_tx(scenario, validator);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let ctx = test_scenario::ctx(scenario);
        let stake_amount = next_u64_in_range(&mut state.random, MAX_DELEGATION_AMOUNT);
        sui_system::request_add_stake(
            &mut system_state, mint_for_testing<SUI>(stake_amount, ctx), ctx);
        test_scenario::return_shared(system_state);
        // end_transaction();
        test_scenario::next_tx(scenario, validator);
        let stake_id = option::destroy_some(most_recent_id_for_address<Stake>(validator));
        vec_map::insert(&mut state.validator_stake_ids, stake_id, validator);
        true
    }

    fun request_withdraw_stake_random(state: &mut TestState): bool {
        
        if (vec_map::is_empty(&mut state.validator_stake_ids)) {
            return false
        };
        let scenario = &mut state.scenario;
        let num_stakes = vec_map::size(&state.validator_stake_ids);
        let idx = next_u64_in_range(&mut state.random, num_stakes);
        let (stake_id, validator) = vec_map::remove_entry_by_idx(&mut state.validator_stake_ids, idx);
        
        test_scenario::next_tx(scenario, validator);
        let stake = take_from_sender_by_id<Stake>(scenario, stake_id);
        let stake_balance = stake::value(&stake);
        if (stake_balance == 0) { 
            test_scenario::return_to_sender<Stake>(scenario, stake);
            return false; 
        };
        let amount_to_withdraw = next_u64_in_range(&mut state.random, stake_balance) + 1;
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        sui_system::request_withdraw_stake(
            &mut system_state, &mut stake, amount_to_withdraw, test_scenario::ctx(scenario));
        test_scenario::return_to_sender<Stake>(scenario, stake);
        test_scenario::return_shared(system_state);
        test_scenario::next_tx(scenario, validator);
        true
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
        // end_transaction();
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
        // end_transaction();
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
        // end_transaction();
        test_scenario::next_tx(scenario, delegator);

        let staked_sui_id = option::destroy_some(most_recent_id_for_address<StakedSui>(delegator));
        // vector::push_back(&mut state.delegation_requests_this_epoch, StakedSuiInfo { delegator, staked_sui_id });
        // std::debug::print(&staked_sui_id);
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
        let delegation = take_from_sender_by_id<Delegation>(scenario, delegation_id);
        let staked_sui = take_from_sender_by_id<StakedSui>(scenario, staked_sui_id(&delegation));
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        sui_system::request_withdraw_delegation(
            &mut system_state, delegation, staked_sui, test_scenario::ctx(scenario));
        test_scenario::return_shared(system_state);
        state.delegation_withdraws_this_epoch = state.delegation_withdraws_this_epoch + 1;
        true
    }

    /// Pick a random delegation requested this epoch and cancel it.
    /// Return false if no delegation request is left in the state.
    fun cancel_delegation_request_random(state: &mut TestState): bool {
        let requests = &mut state.delegation_requests_this_epoch;
        let scenario = &mut state.scenario;
        if (vec_map::is_empty(requests)) {
            return false
        };
        let idx = next_u64_in_range(&mut state.random, vec_map::size(requests));
        // let StakedSuiInfo { staked_sui_id, delegator } = vector::remove(requests, idx);
        let (staked_sui_id, delegator) = vec_map::remove_entry_by_idx(requests, idx);
        // vec_set::insert(&mut state.cancelled_requests, staked_sui_id);
        // std::debug::print(&StakedSuiInfo { staked_sui_id, delegator });
        test_scenario::next_tx(scenario, delegator);
        let staked_sui = take_from_sender_by_id<StakedSui>(scenario, staked_sui_id);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        sui_system::cancel_delegation_request(&mut system_state, staked_sui, test_scenario::ctx(scenario));
        test_scenario::return_shared(system_state);
        // end_transaction();
        test_scenario::next_tx(scenario, delegator);
        true
    }

    /// Pick a random existing delegation and switch it to another random validator.
    /// Return false if no delegation is left in the state.
    fun request_switch_delegation_random(state: &mut TestState): bool {
        if (vec_map::is_empty(&mut state.delegations)) {
            return false
        };
        let scenario = &mut state.scenario;
        let num_delegations = vec_map::size(&state.delegations);
        let idx = next_u64_in_range(&mut state.random, num_delegations);
        let (delegation_id, delegator) = vec_map::remove_entry_by_idx(&mut state.delegations, idx);
        
        test_scenario::next_tx(scenario, delegator);
        let delegation = take_from_sender_by_id<Delegation>(scenario, delegation_id);
        let staked_sui = take_from_sender_by_id<StakedSui>(scenario, staked_sui_id(&delegation));
        let new_validator = random_validator_except(validator_address(&staked_sui), state);

        scenario = &mut state.scenario;
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        sui_system::request_switch_delegation(
            &mut system_state, delegation, staked_sui, new_validator, test_scenario::ctx(scenario));
        test_scenario::return_shared(system_state);
        // state.delegation_switches_this_epoch = state.delegation_switches_this_epoch + 1;
        true
    }

    fun advance_epoch_with_random_rewards(state: &mut TestState) {
        let scenario = &mut state.scenario;
        let rand = &mut state.random;

        test_scenario::next_epoch(scenario, @0x0);
        let new_epoch = tx_context::epoch(test_scenario::ctx(scenario));

        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let storage_charge = next_u64_in_range(rand, MAX_STORAGE_REWARD_AMOUNT_PER_EPOCH);
        let computation_charge = next_u64_in_range(rand, MAX_COMPUTATION_REWARD_AMOUNT_PER_EPOCH);
        let max_storage_rebate = storage_fund_balance(&system_state) + storage_charge;
        let storage_rebate = next_u64_in_range(rand, max_storage_rebate);
        // call advance epoch txn
        sui_system::advance_epoch(
            &mut system_state,
            new_epoch,
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
        let txn_effect = end_transaction();
        let new_objects = test_scenario::transferred_to_account(&txn_effect);
        let num_new_delegation_objs = 0;
        let num_new_coin_objs = 0;

        // filter out the new Delegation objects created during epoch change
        // and add them to `state.delegations`
        while (!vec_map::is_empty(&new_objects)) {
            let (id, owner) = vec_map::pop(&mut new_objects);
            if (is_owned_by_address_of_type<Delegation>(owner, id)) {
                vec_map::insert(&mut state.delegations, id, owner);
                num_new_delegation_objs = num_new_delegation_objs + 1;
            } else if (is_owned_by_address_of_type<Coin<SUI>>(owner, id)) {
                num_new_coin_objs = num_new_coin_objs + 1;
            }
        };
        // std::debug::print(&num_new_delegation_objs);
        // std::debug::print(&num_new_coin_objs);
        // assert!(num_new_delegation_objs == vector::length(&state.delegation_requests_this_epoch), 0);
        // assert!(num_new_coin_objs == state.delegation_withdraws_this_epoch, 0);
        // empty delegation_requests_this_epoch and delegation_withdraws_this_epoch
        state.delegation_requests_this_epoch = vec_map::empty();
        state.delegation_withdraws_this_epoch = 0;
        state.cancelled_requests = vec_set::empty();
    }

    fun begin(seed: vector<u8>): TestState {
        let scenario = test_scenario::begin(@0x0);
        let validators = vector[];
        let validator_stakes = vector[];
        let random = test_random::new(seed);
        let supply_amount = 0;
        let i = 0;
        while (i < NUM_VALIDATORS) {
            let stake = next_u64_in_range(&mut random, MAX_INIT_VALIDATOR_STAKE);
            vector::push_back(&mut validator_stakes, stake);
            vector::push_back(&mut validators, address::from_u256((i as u256)));
            supply_amount = supply_amount + stake;
            i = i + 1;
        };
        let init_storage_fund = next_u64_in_range(&mut random, MAX_INIT_STORAGE_FUND);
        supply_amount = supply_amount + init_storage_fund;
        create_sui_system_state_for_testing(
            create_validators_with_stakes(validator_stakes, test_scenario::ctx(&mut scenario)),
            supply_amount,
            init_storage_fund
        );
        // end_transaction();
        TestState {
            scenario,
            validators,
            validator_stake_ids: vec_map::empty(),
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
            validator_stake_ids: _,
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