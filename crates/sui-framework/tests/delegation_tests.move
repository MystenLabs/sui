// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::delegation_tests {
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::test_scenario::{Self, Scenario};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::staking_pool::{Self, Delegation, StakedSui};
    use std::vector;

    use sui::governance_test_utils::{
        Self,
        create_validator_for_testing,
        create_sui_system_state_for_testing,
    };

    const VALIDATOR_ADDR_1: address = @0x1;
    const VALIDATOR_ADDR_2: address = @0x2;

    const DELEGATOR_ADDR_1: address = @0x42;
    const DELEGATOR_ADDR_2: address = @0x43;

    #[test]
    fun test_add_remove_delegation_flow() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = &mut system_state;

            let ctx = test_scenario::ctx(scenario);

            // Create a delegation to VALIDATOR_ADDR_1.
            sui_system::request_add_delegation(
                system_state_mut_ref, coin::mint_for_testing(60, ctx), VALIDATOR_ADDR_1, ctx);

            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_1) == 0, 101);
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_2) == 0, 102);

            test_scenario::return_shared(system_state);
        };

        governance_test_utils::advance_epoch(scenario);
        
        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {

            let delegation = test_scenario::take_from_sender<Delegation>(scenario);
            assert!(staking_pool::delegation_token_amount(&delegation) == 60, 105);

            let staked_sui = test_scenario::take_from_sender<StakedSui>(scenario);
            assert!(staking_pool::staked_sui_amount(&staked_sui) == 60, 105);


            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = &mut system_state;

            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_1) == 60, 103);
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_2) == 0, 104);

            let ctx = test_scenario::ctx(scenario);

            // Undelegate from VALIDATOR_ADDR_1
            sui_system::request_withdraw_delegation(
                system_state_mut_ref, delegation, staked_sui, ctx);

            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_1) == 60, 107);            
            test_scenario::return_shared(system_state);
        };

        governance_test_utils::advance_epoch(scenario);

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            assert!(sui_system::validator_delegate_amount(&mut system_state, VALIDATOR_ADDR_1) == 0, 107);
            test_scenario::return_shared(system_state);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_switch_delegation() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

            let ctx = test_scenario::ctx(scenario);

            // Create a delegation to VALIDATOR_ADDR_1.
            sui_system::request_add_delegation(
                &mut system_state, coin::mint_for_testing(50, ctx), VALIDATOR_ADDR_1, ctx);

            test_scenario::return_shared(system_state);
        };

        // Advance the epoch so the delegation is activated.
        governance_test_utils::advance_epoch(scenario);
        // Advance epoch one more time to distribute some rewards.
        // The delegator should get 7 SUI of rewards.
        governance_test_utils::advance_epoch_with_reward_amounts(0, 49, scenario);
        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            
            let delegation = test_scenario::take_from_sender<Delegation>(scenario);
            let staked_sui = test_scenario::take_from_sender<StakedSui>(scenario);
            
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

            let ctx = test_scenario::ctx(scenario);

            // Switch stake from VALIDATOR_ADDR_1 to VALIDATOR_ADDR_2
            sui_system::request_switch_delegation(
                &mut system_state, delegation, staked_sui, VALIDATOR_ADDR_2, ctx);
            test_scenario::return_shared(system_state);
        };

        // The delegator should get another 9 SUI of rewards, in total 16 SUI of rewards.
        governance_test_utils::advance_epoch_with_reward_amounts(0, 57, scenario);
        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let staked_sui_ids = test_scenario::ids_for_sender<StakedSui>(scenario);
            assert!(vector::length(&staked_sui_ids) == 2, 0);
            let staked_sui_0 = test_scenario::take_from_sender_by_id(scenario, *vector::borrow(&staked_sui_ids, 0));
            assert!(staking_pool::staked_sui_amount(&staked_sui_0) == 50, 106); // principal with the new validator
            let staked_sui_1 = test_scenario::take_from_sender_by_id(scenario, *vector::borrow(&staked_sui_ids, 1));
            assert!(staking_pool::staked_sui_amount(&staked_sui_1) == 16, 106); // rewards (16 SUI) with the new validator
            test_scenario::return_to_sender(scenario, staked_sui_0);
            test_scenario::return_to_sender(scenario, staked_sui_1);
        };

        governance_test_utils::advance_epoch(scenario);
        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

            // Check that the delegate amounts have been changed successfully.
            assert!(sui_system::validator_delegate_amount(&system_state, VALIDATOR_ADDR_1) == 0, 107);
            assert!(sui_system::validator_delegate_amount(&system_state, VALIDATOR_ADDR_2) == 66, 107);
            test_scenario::return_shared(system_state);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_switch_delegation_zero_rewards() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            let ctx = test_scenario::ctx(scenario);
            // Create a delegation to VALIDATOR_ADDR_1.
            sui_system::request_add_delegation(
                &mut system_state, coin::mint_for_testing(50, ctx), VALIDATOR_ADDR_1, ctx);
            test_scenario::return_shared(system_state);
        };

        // Advance the epoch with no rewards.
        governance_test_utils::advance_epoch(scenario);
        
        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        { 
            let delegation = test_scenario::take_from_sender<Delegation>(scenario);
            let staked_sui = test_scenario::take_from_sender<StakedSui>(scenario);
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

            let ctx = test_scenario::ctx(scenario);

            // Switch stake from VALIDATOR_ADDR_1 to VALIDATOR_ADDR_2
            sui_system::request_switch_delegation(
                &mut system_state, delegation, staked_sui, VALIDATOR_ADDR_2, ctx);
            test_scenario::return_shared(system_state);
        };

        governance_test_utils::advance_epoch(scenario);
        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let staked_sui_ids = test_scenario::ids_for_sender<StakedSui>(scenario);
            // the delegator got no rewards from the previous validator so she
            // didn't get any additional StakedSui object.
            assert!(vector::length(&staked_sui_ids) == 1, 0);
            let staked_sui = test_scenario::take_from_sender_by_id(scenario, *vector::borrow(&staked_sui_ids, 0));
            assert!(staking_pool::staked_sui_amount(&staked_sui) == 50, 0); 
            assert!(staking_pool::validator_address(&staked_sui) == VALIDATOR_ADDR_2, 0); 
            test_scenario::return_to_sender(scenario, staked_sui);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_cancel_delegation_request() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

            let ctx = test_scenario::ctx(scenario);

            // Create a delegation to VALIDATOR_ADDR_1.
            sui_system::request_add_delegation(
                &mut system_state, coin::mint_for_testing(40, ctx), VALIDATOR_ADDR_1, ctx);

            test_scenario::return_shared(system_state);
        };

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_2);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

            let ctx = test_scenario::ctx(scenario);

            // Create another delegation to VALIDATOR_ADDR_1.
            sui_system::request_add_delegation(
                &mut system_state, coin::mint_for_testing(60, ctx), VALIDATOR_ADDR_1, ctx);

            test_scenario::return_shared(system_state);
        };

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            let staked_sui = test_scenario::take_from_sender<StakedSui>(scenario);

            let ctx = test_scenario::ctx(scenario);

            // Now cancel the first one.
            sui_system::cancel_delegation_request(
                &mut system_state, staked_sui, ctx);

            test_scenario::return_shared(system_state);
        };

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            // Check that we have the coin back.
            assert!(coin::value(&coin) == 40, 100);
            test_scenario::return_to_sender(scenario, coin);
        };

        governance_test_utils::advance_epoch(scenario);
        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

            // Check that the delegate amounts have been changed successfully.
            assert!(sui_system::validator_delegate_amount(&system_state, VALIDATOR_ADDR_1) == 60, 101);
            test_scenario::return_shared(system_state);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = sui::sui_system::ESTAKED_SUI_FROM_WRONG_EPOCH)]
    fun test_cancel_delegation_abort() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

            let ctx = test_scenario::ctx(scenario);

            // Create a delegation to VALIDATOR_ADDR_1.
            sui_system::request_add_delegation(
                &mut system_state, coin::mint_for_testing(40, ctx), VALIDATOR_ADDR_1, ctx);

            test_scenario::return_shared(system_state);
        };

        // advance the epoch
        governance_test_utils::advance_epoch(scenario);

        test_scenario::next_tx(scenario, DELEGATOR_ADDR_1);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            let staked_sui = test_scenario::take_from_sender<StakedSui>(scenario);

            let ctx = test_scenario::ctx(scenario);

            // Cancellation should fail since we are no longer in the same epoch.
            sui_system::cancel_delegation_request(
                &mut system_state, staked_sui, ctx);

            test_scenario::return_shared(system_state);
        };

        test_scenario::end(scenario_val);
    }

    fun set_up_sui_system_state(scenario: &mut Scenario) {
        let ctx = test_scenario::ctx(scenario);

        let validators = vector[
            create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_2, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 300, 100);
    }
}
