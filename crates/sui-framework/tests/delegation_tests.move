// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::delegation_tests {
    use sui::coin;
    use sui::epoch_reward_record::EpochRewardRecord;
    use sui::test_scenario::{Self, Scenario};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::delegation::{Self, Delegation};
    use sui::governance_test_utils::{
        Self, 
        create_validator_for_testing, 
        create_sui_system_state_for_testing
    };

    const VALIDATOR_ADDR_1: address = @0x1;
    const VALIDATOR_ADDR_2: address = @0x2;

    const DELEGATOR_ADDR_1: address = @0x42;
    const DELEGATOR_ADDR_2: address = @0x43;

    #[test]
    fun test_add_remove_delegation_flow() {
        let scenario = &mut test_scenario::begin(&VALIDATOR_ADDR_1);
        set_up_sui_system_state(scenario);

        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);

            let ctx = test_scenario::ctx(scenario);

            // Create two delegations to VALIDATOR_ADDR_1.
            sui_system::request_add_delegation(
                system_state_mut_ref, coin::mint_for_testing(10, ctx), VALIDATOR_ADDR_1, ctx);
            sui_system::request_add_delegation(
                system_state_mut_ref, coin::mint_for_testing(60, ctx), VALIDATOR_ADDR_1, ctx);

            // Advance the epoch so that the delegation changes can take into effect.
            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);

            // Check that the delegation amount and count are changed correctly.
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_1) == 70, 101);
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_2) == 0, 102);
            assert!(sui_system::validator_delegator_count(system_state_mut_ref, VALIDATOR_ADDR_1) == 2, 103);
            assert!(sui_system::validator_delegator_count(system_state_mut_ref, VALIDATOR_ADDR_2) == 0, 104);
            test_scenario::return_shared(scenario, system_state_wrapper);
        };

        
        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            
            let delegation = test_scenario::take_last_created_owned<Delegation>(scenario);
            assert!(delegation::delegate_amount(&delegation) == 60, 105);

            
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);

            let ctx = test_scenario::ctx(scenario);

            // Undelegate 60 SUIs from VALIDATOR_ADDR_1
            sui_system::request_remove_delegation(
                system_state_mut_ref, &mut delegation, ctx);

            // Check that the delegation object indeed becomes inactive.
            assert!(!delegation::is_active(&delegation), 106);
            test_scenario::return_owned(scenario, delegation);

            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);

            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_1) == 10, 107);
            assert!(sui_system::validator_delegator_count(system_state_mut_ref, VALIDATOR_ADDR_1) == 1, 108);
            test_scenario::return_shared(scenario, system_state_wrapper);
        };
    }

    #[test]
    fun test_switch_delegation_flow() {
        let scenario = &mut test_scenario::begin(&VALIDATOR_ADDR_1);
        set_up_sui_system_state(scenario);

        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);

            let ctx = test_scenario::ctx(scenario);

            // Create one delegation to VALIDATOR_ADDR_1, and one to VALIDATOR_ADDR_2.
            sui_system::request_add_delegation(
                system_state_mut_ref, coin::mint_for_testing(60, ctx), VALIDATOR_ADDR_1, ctx);
            sui_system::request_add_delegation(
                system_state_mut_ref, coin::mint_for_testing(10, ctx), VALIDATOR_ADDR_2, ctx);

            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);
            test_scenario::return_shared(scenario, system_state_wrapper);
        };

        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            
            let delegation = test_scenario::take_last_created_owned<Delegation>(scenario);
            
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);

            let ctx = test_scenario::ctx(scenario);

            // Switch the 10 SUI delegation from VALIDATOR_ADDR_2 to VALIDATOR_ADDR_1
            sui_system::request_switch_delegation(
                system_state_mut_ref, &mut delegation, VALIDATOR_ADDR_1, ctx);

            test_scenario::return_owned(scenario, delegation);

            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);

            // Check that now VALIDATOR_ADDR_1 has all the delegations
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_1) == 70, 101);
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_2) == 0, 102);
            assert!(sui_system::validator_delegator_count(system_state_mut_ref, VALIDATOR_ADDR_1) == 2, 103);
            assert!(sui_system::validator_delegator_count(system_state_mut_ref, VALIDATOR_ADDR_2) == 0, 104);
            test_scenario::return_shared(scenario, system_state_wrapper);
        };
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_double_claim_reward_active() {
        let scenario = &mut test_scenario::begin(&VALIDATOR_ADDR_1);
        let ctx = test_scenario::ctx(scenario);
        create_sui_system_state_for_testing(
            vector[create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx)], 300, 100);

        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);

            let ctx = test_scenario::ctx(scenario);

            sui_system::request_add_delegation(
                system_state_mut_ref, coin::mint_for_testing(10, ctx), VALIDATOR_ADDR_1, ctx);

            // Advance the epoch twice so that the delegation and rewards can take into effect.
            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);
            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);
            test_scenario::return_shared(scenario, system_state_wrapper);
        };

        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            let delegation = test_scenario::take_last_created_owned<Delegation>(scenario);
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);
            let epoch_reward_record_wrapper = test_scenario::take_last_created_shared<EpochRewardRecord>(scenario);
            let epoch_reward_record_ref = test_scenario::borrow_mut(&mut epoch_reward_record_wrapper);
            let ctx = test_scenario::ctx(scenario);

            sui_system::claim_delegation_reward(system_state_mut_ref, &mut delegation, epoch_reward_record_ref, ctx);

            // We are claiming the same reward twice so this call should fail.
            sui_system::claim_delegation_reward(system_state_mut_ref, &mut delegation, epoch_reward_record_ref, ctx);

            test_scenario::return_owned(scenario, delegation);
            test_scenario::return_shared(scenario, epoch_reward_record_wrapper);
            test_scenario::return_shared(scenario, system_state_wrapper);
        }

    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_double_claim_reward_inactive() {
        let scenario = &mut test_scenario::begin(&VALIDATOR_ADDR_1);
        let ctx = test_scenario::ctx(scenario);
        create_sui_system_state_for_testing(
            vector[create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx)], 300, 100);

        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);

            let ctx = test_scenario::ctx(scenario);

            sui_system::request_add_delegation(
                system_state_mut_ref, coin::mint_for_testing(10, ctx), VALIDATOR_ADDR_1, ctx);
            sui_system::request_add_delegation(
                system_state_mut_ref, coin::mint_for_testing(20, ctx), VALIDATOR_ADDR_1, ctx);

            // Advance the epoch twice so that the delegation and rewards can take into effect.
            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);
            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);
            test_scenario::return_shared(scenario, system_state_wrapper);
        };

        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            let delegation = test_scenario::take_last_created_owned<Delegation>(scenario);
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);
            let epoch_reward_record_wrapper = test_scenario::take_last_created_shared<EpochRewardRecord>(scenario);
            let epoch_reward_record_ref = test_scenario::borrow_mut(&mut epoch_reward_record_wrapper);
            let ctx = test_scenario::ctx(scenario);

            // Remove delegation. Rewards claiming should still work.
            sui_system::request_remove_delegation(system_state_mut_ref, &mut delegation, ctx);
            sui_system::claim_delegation_reward(system_state_mut_ref, &mut delegation, epoch_reward_record_ref, ctx);

            // We are claiming the same reward twice so this call should fail.
            sui_system::claim_delegation_reward(system_state_mut_ref, &mut delegation, epoch_reward_record_ref, ctx);

            test_scenario::return_owned(scenario, delegation);
            test_scenario::return_shared(scenario, epoch_reward_record_wrapper);
            test_scenario::return_shared(scenario, system_state_wrapper);
        };

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
