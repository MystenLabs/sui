// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::stake_tests {
    use sui::coin;
    use sui::test_scenario;
    use sui_system::sui_system::SuiSystemState;
    use sui_system::staking_pool::{Self, StakedSui, PoolTokenExchangeRate};
    use sui::test_utils::assert_eq;
    use sui_system::validator_set;
    use sui::test_utils;
    use sui::table::Table;

    use sui_system::governance_test_utils::{
        add_validator,
        add_validator_candidate,
        advance_epoch,
        advance_epoch_with_reward_amounts,
        assert_validator_total_stake_amounts,
        create_validator_for_testing,
        create_sui_system_state_for_testing,
        stake_with,
        remove_validator,
        remove_validator_candidate,
        total_sui_balance,
        unstake,
    };

    const VALIDATOR_ADDR_1: address = @0x1;
    const VALIDATOR_ADDR_2: address = @0x2;

    const STAKER_ADDR_1: address = @0x42;
    const STAKER_ADDR_2: address = @0x43;
    const STAKER_ADDR_3: address = @0x44;

    const NEW_VALIDATOR_ADDR: address = @0x1a4623343cd42be47d67314fce0ad042f3c82685544bc91d8c11d24e74ba7357;
    // Generated with seed [0;32]
    const NEW_VALIDATOR_PUBKEY: vector<u8> = x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    // Generated using [fn test_proof_of_possession]
    const NEW_VALIDATOR_POP: vector<u8> = x"8b93fc1b33379e2796d361c4056f0f04ad5aea7f4a8c02eaac57340ff09b6dc158eb1945eece103319167f420daf0cb3";

    const MIST_PER_SUI: u64 = 1_000_000_000;

    #[test]
    fun test_split_join_staked_sui() {
        // All this is just to generate a dummy StakedSui object to split and join later
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(STAKER_ADDR_1);
        let scenario = &mut scenario_val;
        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 60, scenario);

        scenario.next_tx(STAKER_ADDR_1);
        {
            let mut staked_sui = scenario.take_from_sender<StakedSui>();
            let ctx = scenario.ctx();
            staked_sui.split_to_sender(20 * MIST_PER_SUI, ctx);
            scenario.return_to_sender(staked_sui);
        };

        // Verify the correctness of the split and send the join txn
        scenario.next_tx(STAKER_ADDR_1);
        {
            let staked_sui_ids = scenario.ids_for_sender<StakedSui>();
            assert!(staked_sui_ids.length() == 2); // staked sui split to 2 coins

            let mut part1 = scenario.take_from_sender_by_id<StakedSui>(staked_sui_ids[0]);
            let part2 = scenario.take_from_sender_by_id<StakedSui>(staked_sui_ids[1]);

            let amount1 = part1.amount();
            let amount2 = part2.amount();
            assert!(amount1 == 20 * MIST_PER_SUI || amount1 == 40 * MIST_PER_SUI);
            assert!(amount2 == 20 * MIST_PER_SUI || amount2 == 40 * MIST_PER_SUI);
            assert!(amount1 + amount2 == 60 * MIST_PER_SUI);

            part1.join(part2);
            assert!(part1.amount() == 60 * MIST_PER_SUI);
            scenario.return_to_sender(part1);
        };
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = staking_pool::EIncompatibleStakedSui)]
    fun test_join_different_epochs() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(STAKER_ADDR_1);
        let scenario = &mut scenario_val;
        // Create two instances of staked sui w/ different epoch activations
        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 60, scenario);
        advance_epoch(scenario);
        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 60, scenario);

        // Verify that these cannot be merged
        scenario.next_tx(STAKER_ADDR_1);
        {
            let staked_sui_ids = scenario.ids_for_sender<StakedSui>();
            let mut part1 = scenario.take_from_sender_by_id<StakedSui>(staked_sui_ids[0]);
            let part2 = scenario.take_from_sender_by_id<StakedSui>(staked_sui_ids[1]);

            part1.join(part2);

            scenario.return_to_sender(part1);
        };
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = staking_pool::EStakedSuiBelowThreshold)]
    fun test_split_below_threshold() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(STAKER_ADDR_1);
        let scenario = &mut scenario_val;
        // Stake 2 SUI
        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 2, scenario);

        scenario.next_tx(STAKER_ADDR_1);
        {
            let mut staked_sui = scenario.take_from_sender<StakedSui>();
            let ctx = scenario.ctx();
            // The remaining amount after splitting is below the threshold so this should fail.
            staked_sui.split_to_sender(1 * MIST_PER_SUI + 1, ctx);
            scenario.return_to_sender(staked_sui);
        };
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = staking_pool::EStakedSuiBelowThreshold)]
    fun test_split_nonentry_below_threshold() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(STAKER_ADDR_1);
        let scenario = &mut scenario_val;
        // Stake 2 SUI
        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 2, scenario);

        scenario.next_tx(STAKER_ADDR_1);
        {
            let mut staked_sui = scenario.take_from_sender<StakedSui>();
            let ctx = scenario.ctx();
            // The remaining amount after splitting is below the threshold so this should fail.
            let stake = staked_sui.split(1 * MIST_PER_SUI + 1, ctx);
            test_utils::destroy(stake);
            scenario.return_to_sender(staked_sui);
        };
        scenario_val.end();
    }

    #[test]
    fun test_add_remove_stake_flow() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;

        scenario.next_tx(STAKER_ADDR_1);
        {
            let mut system_state = scenario.take_shared<SuiSystemState>();
            let system_state_mut_ref = &mut system_state;

            let ctx = scenario.ctx();

            // Create a stake to VALIDATOR_ADDR_1.
            system_state_mut_ref.request_add_stake(
                coin::mint_for_testing(60 * MIST_PER_SUI, ctx), VALIDATOR_ADDR_1, ctx
            );

            assert!(system_state_mut_ref.validator_stake_amount(VALIDATOR_ADDR_1) == 100 * MIST_PER_SUI);
            assert!(system_state_mut_ref.validator_stake_amount(VALIDATOR_ADDR_2) == 100 * MIST_PER_SUI);

            test_scenario::return_shared(system_state);
        };

        advance_epoch(scenario);

        scenario.next_tx(STAKER_ADDR_1);
        {

            let staked_sui = scenario.take_from_sender<StakedSui>();
            assert!(staked_sui.amount() == 60 * MIST_PER_SUI);


            let mut system_state = scenario.take_shared<SuiSystemState>();
            let system_state_mut_ref = &mut system_state;

            assert!(system_state_mut_ref.validator_stake_amount(VALIDATOR_ADDR_1) == 160 * MIST_PER_SUI);
            assert!(system_state_mut_ref.validator_stake_amount(VALIDATOR_ADDR_2) == 100 * MIST_PER_SUI);

            let ctx = scenario.ctx();

            // Unstake from VALIDATOR_ADDR_1
            system_state_mut_ref.request_withdraw_stake(staked_sui, ctx);

            assert!(system_state_mut_ref.validator_stake_amount(VALIDATOR_ADDR_1) == 160 * MIST_PER_SUI);
            test_scenario::return_shared(system_state);
        };

        advance_epoch(scenario);

        scenario.next_tx(STAKER_ADDR_1);
        {
            let mut system_state = scenario.take_shared<SuiSystemState>();
            assert!(system_state.validator_stake_amount(VALIDATOR_ADDR_1) == 100 * MIST_PER_SUI);
            test_scenario::return_shared(system_state);
        };
        scenario_val.end();
    }

    #[test]
    fun test_remove_stake_post_active_flow_no_rewards() {
        test_remove_stake_post_active_flow(false)
    }

    #[test]
    fun test_remove_stake_post_active_flow_with_rewards() {
        test_remove_stake_post_active_flow(true)
    }

    fun test_remove_stake_post_active_flow(should_distribute_rewards: bool) {
        set_up_sui_system_state_with_storage_fund();
        let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;

        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 100, scenario);

        advance_epoch(scenario);

        assert_validator_total_stake_amounts(
            vector[VALIDATOR_ADDR_1, VALIDATOR_ADDR_2],
            vector[200 * MIST_PER_SUI, 100 * MIST_PER_SUI],
            scenario
        );

        if (should_distribute_rewards) {
            // Each validator pool gets 30 MIST and each validator gets an additional 10 MIST.
            advance_epoch_with_reward_amounts(0, 80, scenario);
        } else {
            advance_epoch(scenario);
        };

        remove_validator(VALIDATOR_ADDR_1, scenario);

        advance_epoch(scenario);

        let reward_amt = if (should_distribute_rewards) 15 * MIST_PER_SUI else 0;
        let validator_reward_amt = if (should_distribute_rewards) 10 * MIST_PER_SUI else 0;

        // Make sure stake withdrawal happens
        scenario.next_tx(STAKER_ADDR_1);
        {
            let mut system_state = scenario.take_shared<SuiSystemState>();
            let system_state_mut_ref = &mut system_state;

            assert!(!system_state_mut_ref.validators().is_active_validator_by_sui_address(VALIDATOR_ADDR_1));

            let staked_sui = scenario.take_from_sender<StakedSui>();
            assert_eq(staked_sui.amount(), 100 * MIST_PER_SUI);

            // Unstake from VALIDATOR_ADDR_1
            assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 0);
            let ctx = scenario.ctx();
            system_state_mut_ref.request_withdraw_stake(staked_sui, ctx);

            // Make sure they have all of their stake.
            assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 100 * MIST_PER_SUI + reward_amt);

            test_scenario::return_shared(system_state);
        };

        // Validator unstakes now.
        assert_eq(total_sui_balance(VALIDATOR_ADDR_1, scenario), 0);
        unstake(VALIDATOR_ADDR_1, 0, scenario);
        if (should_distribute_rewards) unstake(VALIDATOR_ADDR_1, 0, scenario);

        // Make sure have all of their stake. NB there is no epoch change. This is immediate.
        assert_eq(total_sui_balance(VALIDATOR_ADDR_1, scenario), 100 * MIST_PER_SUI + reward_amt + validator_reward_amt);

        scenario_val.end();
    }

    #[test]
    fun test_earns_rewards_at_last_epoch() {
        set_up_sui_system_state_with_storage_fund();
        let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;

        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 100, scenario);

        advance_epoch(scenario);

        remove_validator(VALIDATOR_ADDR_1, scenario);

        // Add some rewards after the validator requests to leave. Since the validator is still active
        // this epoch, they should get the rewards from this epoch.
        advance_epoch_with_reward_amounts(0, 80, scenario);

        // Each validator pool gets 30 MIST and validators shares the 20 MIST from the storage fund
        // so validator gets another 10 MIST.
        let reward_amt = 15 * MIST_PER_SUI;
        let validator_reward_amt = 10 * MIST_PER_SUI;

        // Make sure stake withdrawal happens
        scenario.next_tx(STAKER_ADDR_1);
        {
            let mut system_state = scenario.take_shared<SuiSystemState>();
            let system_state_mut_ref = &mut system_state;

            let staked_sui = scenario.take_from_sender<StakedSui>();
            assert_eq(staked_sui.amount(), 100 * MIST_PER_SUI);

            // Unstake from VALIDATOR_ADDR_1
            assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 0);
            let ctx = scenario.ctx();
            system_state_mut_ref.request_withdraw_stake(staked_sui, ctx);

            // Make sure they have all of their stake.
            assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 100 * MIST_PER_SUI + reward_amt);

            test_scenario::return_shared(system_state);
        };

        // Validator unstakes now.
        assert_eq(total_sui_balance(VALIDATOR_ADDR_1, scenario), 0);
        unstake(VALIDATOR_ADDR_1, 0, scenario);
        unstake(VALIDATOR_ADDR_1, 0, scenario);

        // Make sure have all of their stake. NB there is no epoch change. This is immediate.
        assert_eq(total_sui_balance(VALIDATOR_ADDR_1, scenario), 100 * MIST_PER_SUI + reward_amt + validator_reward_amt);

        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator_set::ENotAValidator)]
    fun test_add_stake_post_active_flow() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;

        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 100, scenario);

        advance_epoch(scenario);

        remove_validator(VALIDATOR_ADDR_1, scenario);

        advance_epoch(scenario);

        // Make sure the validator is no longer active.
        scenario.next_tx(STAKER_ADDR_1);
        {
            let mut system_state = scenario.take_shared<SuiSystemState>();
            let system_state_mut_ref = &mut system_state;

            assert!(!system_state_mut_ref.validators().is_active_validator_by_sui_address(VALIDATOR_ADDR_1));

            test_scenario::return_shared(system_state);
        };

        // Now try and stake to the old validator/staking pool. This should fail!
        stake_with(STAKER_ADDR_1, VALIDATOR_ADDR_1, 60, scenario);

        scenario_val.end();
    }

    #[test]
    fun test_add_preactive_remove_preactive() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;

        add_validator_candidate(NEW_VALIDATOR_ADDR, b"name5", b"/ip4/127.0.0.1/udp/85", NEW_VALIDATOR_PUBKEY, NEW_VALIDATOR_POP, scenario);

        // Delegate 100 MIST to the preactive validator
        stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);

        // Advance epoch twice with some rewards
        advance_epoch_with_reward_amounts(0, 400, scenario);
        advance_epoch_with_reward_amounts(0, 900, scenario);

        // Unstake from the preactive validator. There should be no rewards earned.
        unstake(STAKER_ADDR_1, 0, scenario);
        assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 100 * MIST_PER_SUI);

        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator_set::ENotAValidator)]
    fun test_add_preactive_remove_pending_failure() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;

        add_validator_candidate(NEW_VALIDATOR_ADDR, b"name4", b"/ip4/127.0.0.1/udp/84", NEW_VALIDATOR_PUBKEY, NEW_VALIDATOR_POP, scenario);

        add_validator(NEW_VALIDATOR_ADDR, scenario);

        // Delegate 100 SUI to the pending validator. This should fail because pending active validators don't accept
        // new stakes or withdraws.
        stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);

        scenario_val.end();
    }

    #[test]
    fun test_add_preactive_remove_active() {
        set_up_sui_system_state_with_storage_fund();
        let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;

        add_validator_candidate(NEW_VALIDATOR_ADDR, b"name3", b"/ip4/127.0.0.1/udp/83", NEW_VALIDATOR_PUBKEY, NEW_VALIDATOR_POP, scenario);

        // Delegate 100 SUI to the preactive validator
        stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);
        advance_epoch_with_reward_amounts(0, 300, scenario);
        // At this point we got the following distribution of stake:
        // V1: 250, V2: 250, storage fund: 100

        stake_with(STAKER_ADDR_2, NEW_VALIDATOR_ADDR, 50, scenario);
        stake_with(STAKER_ADDR_3, NEW_VALIDATOR_ADDR, 100, scenario);

        // Now the preactive becomes active
        add_validator(NEW_VALIDATOR_ADDR, scenario);
        advance_epoch(scenario);

        // At this point we got the following distribution of stake:
        // V1: 250, V2: 250, V3: 250, storage fund: 100

        advance_epoch_with_reward_amounts(0, 85, scenario);

        // staker 1 and 3 unstake from the validator and earns about 2/5 * (85 - 10) * 1/3 = 10 SUI each.
        // Although they stake in different epochs, they earn the same rewards as long as they unstake
        // in the same epoch because the validator was preactive when they staked.
        // So they will both get slightly more than 110 SUI in total balance.
        unstake(STAKER_ADDR_1, 0, scenario);
        assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 110002000000);
        unstake(STAKER_ADDR_3, 0, scenario);
        assert_eq(total_sui_balance(STAKER_ADDR_3, scenario), 110002000000);

        advance_epoch_with_reward_amounts(0, 85, scenario);
        unstake(STAKER_ADDR_2, 0, scenario);
        // staker 2 earns about 5 SUI from the previous epoch and 24-ish from this one
        // so in total she has about 50 + 5 + 24 = 79 SUI.
        assert_eq(total_sui_balance(STAKER_ADDR_2, scenario), 78862939078);

        scenario_val.end();
    }

    #[test]
    fun test_add_preactive_remove_post_active() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;

        add_validator_candidate(NEW_VALIDATOR_ADDR, b"name1", b"/ip4/127.0.0.1/udp/81", NEW_VALIDATOR_PUBKEY, NEW_VALIDATOR_POP, scenario);

        // Delegate 100 SUI to the preactive validator
        stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);

        // Now the preactive becomes active
        add_validator(NEW_VALIDATOR_ADDR, scenario);
        advance_epoch(scenario);

        // staker 1 earns a bit greater than 30 SUI here. A bit greater because the new validator's voting power
        // is slightly greater than 1/3 of the total voting power.
        advance_epoch_with_reward_amounts(0, 90, scenario);

        // And now the validator leaves the validator set.
        remove_validator(NEW_VALIDATOR_ADDR, scenario);

        advance_epoch(scenario);

        unstake(STAKER_ADDR_1, 0, scenario);
        assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 130006000000);

        scenario_val.end();
    }

    #[test]
    fun test_add_preactive_candidate_drop_out() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(VALIDATOR_ADDR_1);
        let scenario = &mut scenario_val;

        add_validator_candidate(NEW_VALIDATOR_ADDR, b"name2", b"/ip4/127.0.0.1/udp/82", NEW_VALIDATOR_PUBKEY, NEW_VALIDATOR_POP, scenario);

        // Delegate 100 MIST to the preactive validator
        stake_with(STAKER_ADDR_1, NEW_VALIDATOR_ADDR, 100, scenario);

        // Advance epoch and give out some rewards. The candidate should get nothing, of course.
        advance_epoch_with_reward_amounts(0, 800, scenario);

        // Now the candidate leaves.
        remove_validator_candidate(NEW_VALIDATOR_ADDR, scenario);

        // Advance epoch a few times.
        advance_epoch(scenario);
        advance_epoch(scenario);
        advance_epoch(scenario);

        // Unstake now and the staker should get no rewards.
        unstake(STAKER_ADDR_1, 0, scenario);
        assert_eq(total_sui_balance(STAKER_ADDR_1, scenario), 100 * MIST_PER_SUI);

        scenario_val.end();
    }

    #[test]
    fun test_staking_pool_exchange_rate_getter() {
        set_up_sui_system_state();
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        stake_with(@0x42, @0x2, 100, scenario); // stakes 100 SUI with 0x2
        scenario.next_tx(@0x42);
        let staked_sui = scenario.take_from_address<StakedSui>(@0x42);
        let pool_id = staked_sui.pool_id();
        test_scenario::return_to_address(@0x42, staked_sui);
        advance_epoch(scenario); // advances epoch to effectuate the stake
        // Each staking pool gets 10 SUI of rewards.
        advance_epoch_with_reward_amounts(0, 20, scenario);
        let mut system_state = scenario.take_shared<SuiSystemState>();
        let rates = system_state.pool_exchange_rates(&pool_id);
        assert_eq(rates.length(), 3);
        assert_exchange_rate_eq(rates, 0, 0, 0);     // no tokens at epoch 0
        assert_exchange_rate_eq(rates, 1, 200, 200); // 200 SUI of self + delegate stake at epoch 1
        assert_exchange_rate_eq(rates, 2, 210, 200); // 10 SUI of rewards at epoch 2
        test_scenario::return_shared(system_state);
        scenario_val.end();
    }

    fun assert_exchange_rate_eq(
        rates: &Table<u64, PoolTokenExchangeRate>, epoch: u64, sui_amount: u64, pool_token_amount: u64
    ) {
        let rate = &rates[epoch];
        assert_eq(rate.sui_amount(), sui_amount * MIST_PER_SUI);
        assert_eq(rate.pool_token_amount(), pool_token_amount * MIST_PER_SUI);
    }

    fun set_up_sui_system_state() {
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();

        let validators = vector[
            create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_2, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        scenario_val.end();
    }

    fun set_up_sui_system_state_with_storage_fund() {
        let mut scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();

        let validators = vector[
            create_validator_for_testing(VALIDATOR_ADDR_1, 100, ctx),
            create_validator_for_testing(VALIDATOR_ADDR_2, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 300, 100, ctx);
        scenario_val.end();
    }
}
