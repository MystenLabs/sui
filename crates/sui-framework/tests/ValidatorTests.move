// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::ValidatorTests {
    use Sui::Coin::{Self, Coin};
    use Sui::SUI::SUI;
    use Sui::TestScenario;
    use Sui::Validator;

    #[test]
    fun test_validator_owner_flow() {
        let sender = @0x1;
        let scenario = &mut TestScenario::begin(&sender);
        {
            let ctx = TestScenario::ctx(scenario);

            let init_stake = Coin::into_balance(Coin::mint_for_testing(10, ctx));
            let validator = Validator::new(
                sender,
                x"FF",
                b"Validator1",
                x"FFFF",
                init_stake,
            );
            assert!(Validator::stake_amount(&validator) == 10, 0);
            assert!(Validator::sui_address(&validator) == sender, 0);

            Validator::destroy(validator, ctx);
        };

        // Check that after destroy, the original stake still exists.
        TestScenario::next_tx(scenario, &sender);
        {
            let stake_coin = TestScenario::take_owned<Coin<SUI>>(scenario);
            assert!(Coin::value(&stake_coin) == 10, 0);
            TestScenario::return_owned(scenario, stake_coin);
        };
    }

    #[test]
    fun test_pending_validator_flow() {
        let sender = @0x1;
        let scenario = &mut TestScenario::begin(&sender);
        let ctx = TestScenario::ctx(scenario);
        let init_stake = Coin::into_balance(Coin::mint_for_testing(10, ctx));
        let validator = Validator::new(
            sender,
            x"FF",
            b"Validator1",
            x"FFFF",
            init_stake,
        );

        let new_stake = Coin::into_balance(Coin::mint_for_testing(30, ctx));
        Validator::request_add_stake(&mut validator, new_stake);

        assert!(Validator::stake_amount(&validator) == 10, 0);
        assert!(Validator::pending_stake_amount(&validator) == 30, 0);

        Validator::request_withdraw_stake(&mut validator, 5, 35);
        assert!(Validator::stake_amount(&validator) == 10, 0);
        assert!(Validator::pending_stake_amount(&validator) == 30, 0);
        assert!(Validator::pending_withdraw(&validator) == 5, 0);

        // Calling `adjust_stake` will withdraw the coin and transfer to sender.
        Validator::adjust_stake(&mut validator, ctx);

        assert!(Validator::stake_amount(&validator) == 35, 0);
        assert!(Validator::pending_stake_amount(&validator) == 0, 0);
        assert!(Validator::pending_withdraw(&validator) == 0, 0);

        TestScenario::next_tx(scenario, &sender);
        {
            let withdraw = TestScenario::take_owned<Coin<SUI>>(scenario);
            assert!(Coin::value(&withdraw) == 5, 0);
            TestScenario::return_owned(scenario, withdraw);
        };

        Validator::destroy(validator, TestScenario::ctx(scenario));
    }
}
