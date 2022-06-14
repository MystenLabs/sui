// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::validator_tests {
    use sui::coin::{Self, Coin};
    use sui::SUI::SUI;
    use sui::TestScenario;
    use sui::validator;

    #[test]
    fun test_validator_owner_flow() {
        let sender = @0x1;
        let scenario = &mut TestScenario::begin(&sender);
        {
            let ctx = TestScenario::ctx(scenario);

            let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));
            let validator = validator::new(
                sender,
                x"FF",
                b"Validator1",
                x"FFFF",
                init_stake,
            );
            assert!(validator::stake_amount(&validator) == 10, 0);
            assert!(validator::sui_address(&validator) == sender, 0);

            validator::destroy(validator, ctx);
        };

        // Check that after destroy, the original stake still exists.
        TestScenario::next_tx(scenario, &sender);
        {
            let stake_coin = TestScenario::take_owned<Coin<SUI>>(scenario);
            assert!(coin::value(&stake_coin) == 10, 0);
            TestScenario::return_owned(scenario, stake_coin);
        };
    }

    #[test]
    fun test_pending_validator_flow() {
        let sender = @0x1;
        let scenario = &mut TestScenario::begin(&sender);
        let ctx = TestScenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));
        let validator = validator::new(
            sender,
            x"FF",
            b"Validator1",
            x"FFFF",
            init_stake,
        );

        let new_stake = coin::into_balance(coin::mint_for_testing(30, ctx));
        validator::request_add_stake(&mut validator, new_stake);

        assert!(validator::stake_amount(&validator) == 10, 0);
        assert!(validator::pending_stake_amount(&validator) == 30, 0);

        validator::request_withdraw_stake(&mut validator, 5, 35);
        assert!(validator::stake_amount(&validator) == 10, 0);
        assert!(validator::pending_stake_amount(&validator) == 30, 0);
        assert!(validator::pending_withdraw(&validator) == 5, 0);

        // Calling `adjust_stake` will withdraw the coin and transfer to sender.
        validator::adjust_stake(&mut validator, ctx);

        assert!(validator::stake_amount(&validator) == 35, 0);
        assert!(validator::pending_stake_amount(&validator) == 0, 0);
        assert!(validator::pending_withdraw(&validator) == 0, 0);

        TestScenario::next_tx(scenario, &sender);
        {
            let withdraw = TestScenario::take_owned<Coin<SUI>>(scenario);
            assert!(coin::value(&withdraw) == 5, 0);
            TestScenario::return_owned(scenario, withdraw);
        };

        validator::destroy(validator, TestScenario::ctx(scenario));
    }
}
