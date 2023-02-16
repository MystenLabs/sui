// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::delegation_tests {
    use sui::coin;
    use sui::test_scenario::{Self, Scenario};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::staking_pool::{Self, Delegation, StakedSui};

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

    fun set_up_sui_system_state(scenario: &mut Scenario) {
        let ctx = test_scenario::ctx(scenario);

        let validators = vector[
            create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_2, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 300, 100, ctx);
    }
}
