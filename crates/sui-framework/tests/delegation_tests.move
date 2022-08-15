// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: add tests for checking the maths of reward distribution here, and tests for bonding period too.
#[test_only]
module sui::delegation_tests {
    use sui::coin;
    use sui::test_scenario::{Self, Scenario};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::staking_pool::{Self, Delegation};

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

            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);

            // The amount hasn't changed yet because delegation is not activated
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_1) == 0, 101);
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_2) == 0, 102);
            test_scenario::return_shared(scenario, system_state_wrapper);
        };

        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            let delegation = test_scenario::take_last_created_owned<Delegation>(scenario);
            assert!(staking_pool::delegation_sui_amount(&delegation) == 60, 105);
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);
            
            let ctx = test_scenario::ctx(scenario);

            sui_system::request_activate_delegation(
                system_state_mut_ref, &mut delegation, ctx);

            test_scenario::return_owned(scenario, delegation);

            // Advance the epoch so that the delegation change can take into effect.
            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);

            // Check that the delegation amount is changed correctly.
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_1) == 60, 101);
            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_2) == 0, 102);
            test_scenario::return_shared(scenario, system_state_wrapper);
        };
        
        test_scenario::next_tx(scenario, &DELEGATOR_ADDR_1);
        {
            
            let delegation = test_scenario::take_last_created_owned<Delegation>(scenario);
            assert!(staking_pool::delegation_sui_amount(&delegation) == 60, 105);

            
            let system_state_wrapper = test_scenario::take_shared<SuiSystemState>(scenario);
            let system_state_mut_ref = test_scenario::borrow_mut(&mut system_state_wrapper);

            let ctx = test_scenario::ctx(scenario);

            // Undelegate 40 SUI from VALIDATOR_ADDR_1
            sui_system::request_withdraw_delegation(
                system_state_mut_ref, &mut delegation, 40, ctx);

            // Check that the delegation object indeed becomes inactive.
            assert!(staking_pool::delegation_sui_amount(&delegation) == 20, 106);
            test_scenario::return_owned(scenario, delegation);

            governance_test_utils::advance_epoch(system_state_mut_ref, scenario);

            assert!(sui_system::validator_delegate_amount(system_state_mut_ref, VALIDATOR_ADDR_1) == 20, 107);
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
