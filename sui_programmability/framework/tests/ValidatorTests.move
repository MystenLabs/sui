// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::ValidatorTests {
    use Sui::Coin::{Self, Coin};
    use Sui::ID;
    use Sui::SUI::SUI;
    use Sui::TestScenario;
    use Sui::Validator::{Self, Validator};

    #[test]
    public(script) fun test_validator_owner_flow() {
        let sender = @0x1;
        let scenario = &mut TestScenario::begin(&sender);

        // Create a new validator.
        // Remember the original stake coin ID, which will be used at the end.
        TestScenario::next_tx(scenario, &sender);
        let original_stake_coin_id = {
            let ctx = TestScenario::ctx(scenario);
            let init_stake = Coin::mint_for_testing(10, ctx);
            let id = *ID::id(&init_stake);
            Validator::create(
                init_stake,
                b"Validator1",
                x"FFFF",
                ctx,
            );
            id
        };
        // Add new stake to the validator.
        TestScenario::next_tx(scenario, &sender);
        {
            let validator = TestScenario::take_object<Validator>(scenario);
            assert!(Validator::get_stake_amount(&validator) == 10, 0);
            assert!(Validator::get_sui_address(&validator) == sender, 0);
            let ctx = TestScenario::ctx(scenario);
            let new_stake = Coin::mint_for_testing(30, ctx);
            Validator::add_stake(&mut validator, new_stake, ctx);
            TestScenario::return_object(scenario, validator);
        };
        // Withdraw stake from the validator.
        TestScenario::next_tx(scenario, &sender);
        {
            let validator = TestScenario::take_object<Validator>(scenario);
            assert!(Validator::get_stake_amount(&validator) == 40, 0);
            let ctx = TestScenario::ctx(scenario);
            Validator::withdraw_stake(&mut validator, 5, ctx);
            TestScenario::return_object(scenario, validator);
        };
        // Check that the withdraw coin exists.
        TestScenario::next_tx(scenario, &sender);
        {
            let validator = TestScenario::take_object<Validator>(scenario);
            assert!(Validator::get_stake_amount(&validator) == 35, 0);
            let withdraw_coin = TestScenario::take_object<Coin<SUI>>(scenario);
            assert!(Coin::value(&withdraw_coin) == 5, 0);
            TestScenario::return_object(scenario, withdraw_coin);
            TestScenario::return_object(scenario, validator);
        };
        // Destroy the validator object.
        TestScenario::next_tx(scenario, &sender);
        {
            let validator = TestScenario::take_object<Validator>(scenario);
            let ctx = TestScenario::ctx(scenario);
            Validator::destroy(validator, ctx);
        };
        // Check that after destroy, the original stake still exists.
        TestScenario::next_tx(scenario, &sender);
        {
            let remaining_stake_coin = TestScenario::take_object_by_id<Coin<SUI>>(scenario, original_stake_coin_id);
            assert!(Coin::value(&remaining_stake_coin) == 35, 0);
            TestScenario::return_object(scenario, remaining_stake_coin);
        };
    }

    #[test]
    public(script) fun test_pending_validator_flow() {
        let sender = @0x1;
        let scenario = &mut TestScenario::begin(&sender);

        TestScenario::next_tx(scenario, &sender);
        {
            let ctx = TestScenario::ctx(scenario);
            let init_stake = Coin::mint_for_testing(10, ctx);
            Validator::create(
                init_stake,
                b"Validator1",
                x"FFFF",
                ctx,
            );
        };
        TestScenario::next_tx(scenario, &sender);
        {
            let validator = TestScenario::take_object<Validator>(scenario);
            let ctx = TestScenario::ctx(scenario);
            let new_stake = Coin::mint_for_testing(30, ctx);
            Validator::request_add_stake(&mut validator, new_stake, 100);
            TestScenario::return_object(scenario, validator);
        };
        TestScenario::next_tx(scenario, &sender);
        {
            let validator = TestScenario::take_object<Validator>(scenario);
            assert!(Validator::get_stake_amount(&validator) == 10, 0);
            assert!(Validator::get_pending_stake_amount(&validator) == 30, 0);

            Validator::request_withdraw_stake(&mut validator, 5, 35);
            TestScenario::return_object(scenario, validator);
        };
        TestScenario::next_tx(scenario, &sender);
        {
            let validator = TestScenario::take_object<Validator>(scenario);
            assert!(Validator::get_stake_amount(&validator) == 10, 0);
            assert!(Validator::get_pending_stake_amount(&validator) == 30, 0);
            assert!(Validator::get_pending_withdraw(&validator) == 5, 0);

            let ctx = TestScenario::ctx(scenario);
            // Calling `adjust_stake` will withdraw the coin and transfer to sender.
            Validator::adjust_stake(&mut validator, ctx);
            TestScenario::return_object(scenario, validator);
        };
        TestScenario::next_tx(scenario, &sender);
        {
            let validator = TestScenario::take_object<Validator>(scenario);
            assert!(Validator::get_stake_amount(&validator) == 35, 0);
            assert!(Validator::get_pending_stake_amount(&validator) == 0, 0);
            assert!(Validator::get_pending_withdraw(&validator) == 0, 0);
            TestScenario::return_object(scenario, validator);

            let withdraw = TestScenario::take_object<Coin<SUI>>(scenario);
            assert!(Coin::value(&withdraw) == 5, 0);
            TestScenario::return_object(scenario, withdraw);
        };
    }
}