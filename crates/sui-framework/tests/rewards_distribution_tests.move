// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::rewards_distribution_tests {
    use sui::coin;
    use sui::test_scenario::{Self, Scenario};
    use sui::sui_system::{Self, SuiSystemState};

    use sui::governance_test_utils::{
        Self,
        advance_epoch,
        advance_epoch_with_reward_amounts,
        advance_epoch_with_reward_amounts_and_subsidy_rate,
        advance_epoch_with_reward_amounts_and_slashing_rates,
        assert_validator_delegate_amounts,
        assert_validator_stake_amounts,
        create_validator_for_testing,
        create_sui_system_state_for_testing,
        delegate_to,
        total_sui_balance, undelegate
    };

    const VALIDATOR_ADDR_1: address = @0x1;
    const VALIDATOR_ADDR_2: address = @0x2;
    const VALIDATOR_ADDR_3: address = @0x3;
    const VALIDATOR_ADDR_4: address = @0x4;

    const DELEGATOR_ADDR_1: address = @0x42;
    const DELEGATOR_ADDR_2: address = @0x43;

    #[test]
    fun test_validator_rewards() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        // need to advance epoch so validator's staking starts counting
        governance_test_utils::advance_epoch(scenario);

        advance_epoch_with_reward_amounts(0, 100, scenario);
        assert_validator_stake_amounts(validator_addrs(), vector[110, 220, 330, 440], scenario);

        test_scenario::next_tx(scenario, VALIDATOR_ADDR_2);
        {
            let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
            let ctx = test_scenario::ctx(scenario);
            sui_system::request_add_stake(&mut system_state, coin::mint_for_testing(720, ctx), ctx);
            test_scenario::return_shared(system_state);
        };

        advance_epoch_with_reward_amounts(0, 100, scenario);
        // validator 2's new stake hasn' started counting yet so she only gets 20% of the rewards.
        assert_validator_stake_amounts(validator_addrs(), vector[120, 960, 360, 480], scenario);

        advance_epoch_with_reward_amounts(0, 100, scenario);
        // validator 2's new stake started counting so she gets half of the rewards.
        assert_validator_stake_amounts(validator_addrs(), vector[126, 1010, 378, 505], scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_stake_subsidy() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state_with_big_amounts(scenario);

        // need to advance epoch so validator's staking starts counting
        governance_test_utils::advance_epoch(scenario);

        advance_epoch_with_reward_amounts_and_subsidy_rate(0, 100, 1, scenario);
        assert_validator_stake_amounts(validator_addrs(), vector[100_010_010, 200_020_020, 300_030_030, 400_040_040], scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_delegation_rewards() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        // need to advance epoch so validator's staking starts counting

        delegate_to(DELEGATOR_ADDR_1, VALIDATOR_ADDR_1, 200, scenario);
        delegate_to(DELEGATOR_ADDR_2, VALIDATOR_ADDR_2, 100, scenario);
        governance_test_utils::advance_epoch(scenario);

        // 10 SUI rewards for each 100 SUI of stake
        advance_epoch_with_reward_amounts(0, 130, scenario);
        assert_validator_stake_amounts(validator_addrs(), vector[110, 220, 330, 440], scenario);
        undelegate(DELEGATOR_ADDR_1, 0, 0, scenario);
        delegate_to(DELEGATOR_ADDR_2, VALIDATOR_ADDR_1, 600, scenario);
        // 10 SUI rewards for each 110 SUI of stake
        advance_epoch_with_reward_amounts(0, 130, scenario);
        assert!(total_sui_balance(DELEGATOR_ADDR_1, scenario) == 240, 0); // 40 SUI of rewards received
        assert_validator_stake_amounts(validator_addrs(), vector[120, 240, 360, 480], scenario);
        undelegate(DELEGATOR_ADDR_2, 0, 0, scenario);
        governance_test_utils::advance_epoch(scenario); 
        assert!(total_sui_balance(DELEGATOR_ADDR_2, scenario) == 120, 0); // 20 SUI of rewards received

        // 10 SUI rewards for each 120 SUI of stake
        advance_epoch_with_reward_amounts(0, 150, scenario);
        undelegate(DELEGATOR_ADDR_2, 0, 0, scenario); // unstake 600 principal SUI
        governance_test_utils::advance_epoch(scenario);
        // additional 600 SUI of principal and 50 SUI of rewards withdrawn to Coin<SUI>
        assert!(total_sui_balance(DELEGATOR_ADDR_2, scenario) == 770, 0);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_delegation_tiny_rewards() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state_with_big_amounts(scenario);

        // delegate a large amount
        delegate_to(DELEGATOR_ADDR_1, VALIDATOR_ADDR_1, 200000000, scenario);
        
        governance_test_utils::advance_epoch(scenario);

        advance_epoch_with_reward_amounts(0, 150000, scenario);

        // delegate a small amount
        delegate_to(DELEGATOR_ADDR_1, VALIDATOR_ADDR_1, 10, scenario);
        advance_epoch_with_reward_amounts(0, 130, scenario); 
        
        // undelegate the delegations
        undelegate(DELEGATOR_ADDR_1, 1, 1, scenario);

        // and advance epoch should succeed
        advance_epoch_with_reward_amounts(0, 150, scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_validator_commission() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        delegate_to(DELEGATOR_ADDR_1, VALIDATOR_ADDR_1, 100, scenario);
        delegate_to(DELEGATOR_ADDR_2, VALIDATOR_ADDR_2, 100, scenario);
        governance_test_utils::advance_epoch(scenario);

        set_commission_rate_and_advance_epoch(VALIDATOR_ADDR_2, 5000, scenario); // 50% commission

        // 10 SUI for each 100 SUI staked
        advance_epoch_with_reward_amounts(0, 120, scenario);
        // 5 SUI, or 50 % of delegator_2's rewards, goes to validator_2
        assert_validator_delegate_amounts(validator_addrs(), vector[110, 105, 0, 0], scenario);
        assert_validator_stake_amounts(validator_addrs(), vector[110, 225, 330, 440], scenario);

        set_commission_rate_and_advance_epoch(VALIDATOR_ADDR_1, 1000, scenario); // 10% commission

        // 20 SUI for each 110 SUI staked
        advance_epoch_with_reward_amounts(0, 240, scenario);

        // 2 SUI, or 10 % of delegator_1's rewards (20 SUI), goes to validator_1
        // so delegator_1 now has 110 + 20 - 2 = 128 SUI.
        // And 10 SUI, or 50% of delegator_2's rewards (20 SUI) goes to validator_2
        // so delegator_2 now has 105 +20 - 10 = 115 SUI.
        assert_validator_delegate_amounts(validator_addrs(), vector[128, 115, 0, 0], scenario);

        // validator_1 gets 20 SUI of their own rewards and 2 SUI of commission
        // so in total 110 + 20 + 2 = 132 SUI.
        // validator_2 gets 40 SUI of their own rewards and 10 SUI of commission
        // so in total 225 + 40 + 10 = 275 SUI.
        // validator_3 and validator_4 just get their regular shares (60 SUI and 80 SUI).
        assert_validator_stake_amounts(validator_addrs(), vector[132, 275, 390, 520], scenario);

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_rewards_slashing() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        advance_epoch(scenario);

        delegate_to(DELEGATOR_ADDR_1, VALIDATOR_ADDR_1, 100, scenario);
        delegate_to(DELEGATOR_ADDR_2, VALIDATOR_ADDR_2, 100, scenario);

        advance_epoch(scenario);

        // validator_2 is reported by 3 other validators, so (200 + 300 + 400) / 1200 = 75% of total stake.
        report_validator(VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, scenario);
        report_validator(VALIDATOR_ADDR_3, VALIDATOR_ADDR_2, scenario);
        report_validator(VALIDATOR_ADDR_4, VALIDATOR_ADDR_2, scenario);

        // validator_1 is reported by only 1 other validator, which is 300 / 1200 = 25% of total stake.
        report_validator(VALIDATOR_ADDR_3, VALIDATOR_ADDR_1, scenario);

        // 1200 SUI of total rewards, 50% threshold and 10% reward slashing.
        // So validator_2 is the only one whose rewards should get slashed.
        advance_epoch_with_reward_amounts_and_slashing_rates(
            0, 1200, 1000, scenario
        );

        // Without reward slashing, the validator's stakes should be [200, 400, 600, 800]
        // after the last epoch advancement.
        // Since 20 SUI, or 10% of validator_2's rewards (200) are slashed, she only has 400 - 20 = 380 now.
        // There are in total 30 SUI of rewards slashed (20 from the validator, and 10 from her delegator)
        // so the unslashed validators each get their weighted share of additional rewards, which is
        // 30 / 9 = 3, 30 * 3 / 9 = 10 and 30 * 4 / 9 = 13.
        assert_validator_stake_amounts(validator_addrs(), vector[203, 380, 610, 813], scenario);

        // Undelegate so we can check the delegation rewards as well.
        undelegate(DELEGATOR_ADDR_1, 0, 0, scenario);
        undelegate(DELEGATOR_ADDR_2, 0, 0, scenario);

        advance_epoch(scenario);

        // Same analysis as above. Delegator 1 has 3 additional SUI, and 10% of delegator 2's rewards are slashed.
        assert!(total_sui_balance(DELEGATOR_ADDR_1, scenario) == 203, 0);
        assert!(total_sui_balance(DELEGATOR_ADDR_2, scenario) == 190, 0);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_rewards_slashing_with_storage_fund() {
        let scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;
        set_up_sui_system_state(scenario);

        // Put 300 SUI into the storage fund.
        advance_epoch_with_reward_amounts(300, 0, scenario);

        // Add a few delegations.
        delegate_to(DELEGATOR_ADDR_1, VALIDATOR_ADDR_3, 100, scenario);
        delegate_to(DELEGATOR_ADDR_2, VALIDATOR_ADDR_4, 100, scenario);
        advance_epoch(scenario);

        // validator_4 is reported by 3 other validators, so (100 + 200 + 400) / 1200 = 58% of total stake.
        report_validator(VALIDATOR_ADDR_1, VALIDATOR_ADDR_4, scenario);
        report_validator(VALIDATOR_ADDR_2, VALIDATOR_ADDR_4, scenario);
        report_validator(VALIDATOR_ADDR_3, VALIDATOR_ADDR_4, scenario);

        // 1000 SUI of storage rewards, 1500 SUI of computation rewards, 50% slashing threshold
        // and 20% slashing rate
        advance_epoch_with_reward_amounts_and_slashing_rates(
            1000, 1500, 2000, scenario
        );

        // Validator 1 gets 100 SUI of computation rewards + 75 SUI of storage fund rewards +
        // 14 SUI (1/7) of validator 4's slashed computation reward and 5 SUI (1/3) of validator 4's
        // storage fund reward, so in total it gets 194 SUI of rewards. Same analysis for other 2
        // unslashed validators too.
        // Validator 4 should get 475 SUI of rewards without slashing. 20% is slashed so she gets
        // 380 SUI of rewards.
        assert_validator_stake_amounts(validator_addrs(), vector[294, 508, 722, 780], scenario);

        // Undelegate so we can check the delegation rewards as well.
        undelegate(DELEGATOR_ADDR_1, 0, 0, scenario);
        undelegate(DELEGATOR_ADDR_2, 0, 0, scenario);

        advance_epoch(scenario);

        assert!(total_sui_balance(DELEGATOR_ADDR_1, scenario) == 215, 0);
        assert!(total_sui_balance(DELEGATOR_ADDR_2, scenario) == 180, 0);

        test_scenario::end(scenario_val);
    }

    fun set_up_sui_system_state(scenario: &mut Scenario) {
        let ctx = test_scenario::ctx(scenario);

        let validators = vector[
            create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_2, 200, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_3, 300, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_4, 400, ctx),
        ];
        create_sui_system_state_for_testing(validators, 1000, 0, ctx);
    }

    fun set_up_sui_system_state_with_big_amounts(scenario: &mut Scenario) {
        let ctx = test_scenario::ctx(scenario);

        let validators = vector[
            create_validator_for_testing(VALIDATOR_ADDR_1, 100000000, ctx), 
            create_validator_for_testing(VALIDATOR_ADDR_2, 200000000, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_3, 300000000, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_4, 400000000, ctx),
        ];
        create_sui_system_state_for_testing(validators, 1000000000, 0, ctx);
    }

    fun validator_addrs() : vector<address> {
        vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2, VALIDATOR_ADDR_3, VALIDATOR_ADDR_4]
    }

    fun set_commission_rate_and_advance_epoch(addr: address, commission_rate: u64, scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, addr);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let ctx = test_scenario::ctx(scenario);
        sui_system::request_set_commission_rate(&mut system_state, commission_rate, ctx);
        test_scenario::return_shared(system_state);
        governance_test_utils::advance_epoch(scenario);
    }

    fun report_validator(reporter: address, reportee: address, scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, reporter);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let ctx = test_scenario::ctx(scenario);
        sui_system::report_validator(&mut system_state, reportee, ctx);
        test_scenario::return_shared(system_state);
    }
}
