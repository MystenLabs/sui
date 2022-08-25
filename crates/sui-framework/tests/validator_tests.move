// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::validator_tests {
    use sui::coin;
    use sui::sui::SUI;
    use sui::test_scenario;
    use sui::validator;
    use sui::stake::Stake;
    use sui::locked_coin::{Self, LockedCoin};
    use sui::stake;
    use std::option;

    #[test]
    fun test_validator_owner_flow() {
        let sender = @0xa257c6835e1d2ee0ac6e15a70a7dde97a67977fb;

        let scenario = &mut test_scenario::begin(&sender);
        {
            let ctx = test_scenario::ctx(scenario);

            let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));
            let validator = validator::new(
                sender,
                vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
                vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
                vector[184, 3, 152, 215, 72, 111, 54, 51, 255, 187, 223, 153, 215, 183, 127, 208, 59, 49, 54, 72, 5, 108, 39, 195, 202, 221, 54, 191, 114, 207, 186, 202, 122, 237, 5, 115, 98, 163, 110, 120, 159, 154, 167, 218, 11, 121, 245, 195, 70, 67, 206, 115, 3, 49, 249, 95, 70, 232, 233, 180, 109, 44, 16, 15],
                b"Validator1",
                x"FFFF",
                init_stake,
                option::none(),
                1,
                ctx
            );
            assert!(validator::stake_amount(&validator) == 10, 0);
            assert!(validator::sui_address(&validator) == sender, 0);

            validator::destroy(validator);
        };

        // Check that after destroy, the original stake still exists.
        test_scenario::next_tx(scenario, &sender);
        {
            let stake = test_scenario::take_owned<Stake>(scenario);
            assert!(stake::value(&stake) == 10, 0);
            test_scenario::return_owned(scenario, stake);
        };
    }

    #[test]
    fun test_pending_validator_flow() {
        let sender = @0xa257c6835e1d2ee0ac6e15a70a7dde97a67977fb;
        let scenario = &mut test_scenario::begin(&sender);
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = validator::new(
            sender,
            vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
            vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
            vector[184, 3, 152, 215, 72, 111, 54, 51, 255, 187, 223, 153, 215, 183, 127, 208, 59, 49, 54, 72, 5, 108, 39, 195, 202, 221, 54, 191, 114, 207, 186, 202, 122, 237, 5, 115, 98, 163, 110, 120, 159, 154, 167, 218, 11, 121, 245, 195, 70, 67, 206, 115, 3, 49, 249, 95, 70, 232, 233, 180, 109, 44, 16, 15],
            b"Validator1",
            x"FFFF",
            init_stake,
            option::none(),
            1,
            ctx
        );

        test_scenario::next_tx(scenario, &sender);
        {
            let ctx = test_scenario::ctx(scenario);
            let new_stake = coin::into_balance(coin::mint_for_testing(30, ctx));
            validator::request_add_stake(&mut validator, new_stake, option::none(), ctx);

            assert!(validator::stake_amount(&validator) == 10, 0);
            assert!(validator::pending_stake_amount(&validator) == 30, 0);
        };

        test_scenario::next_tx(scenario, &sender);
        {
            let stake = test_scenario::take_last_created_owned<Stake>(scenario);
            let ctx = test_scenario::ctx(scenario);
            validator::request_withdraw_stake(&mut validator, &mut stake, 5, 35, ctx);
            test_scenario::return_owned(scenario, stake);
            assert!(validator::stake_amount(&validator) == 10, 0);
            assert!(validator::pending_stake_amount(&validator) == 30, 0);
            assert!(validator::pending_withdraw(&validator) == 5, 0);

            // Calling `adjust_stake_and_gas_price` will withdraw the coin and transfer to sender.
            validator::adjust_stake_and_gas_price(&mut validator);

            assert!(validator::stake_amount(&validator) == 35, 0);
            assert!(validator::pending_stake_amount(&validator) == 0, 0);
            assert!(validator::pending_withdraw(&validator) == 0, 0);
        };

        test_scenario::next_tx(scenario, &sender);
        {
            let withdraw = test_scenario::take_owned<LockedCoin<SUI>>(scenario);
            assert!(locked_coin::value(&withdraw) == 5, 0);
            test_scenario::return_owned(scenario, withdraw);
        };

        validator::destroy(validator);
    }
}
