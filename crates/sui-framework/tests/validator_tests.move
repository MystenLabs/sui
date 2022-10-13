// Copyright (c) Mysten Labs, Inc.
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
        let sender = @0x8feebb589ffa14667ff721b7cfb186cfad6530fc;

        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        {
            let ctx = test_scenario::ctx(scenario);

            let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));
            let validator = validator::new(
                sender,
                vector[131, 117, 151, 65, 106, 116, 161, 1, 125, 44, 138, 143, 162, 193, 244, 241, 19, 159, 175, 120, 76, 35, 83, 213, 49, 79, 36, 21, 121, 79, 86, 242, 16, 1, 185, 176, 31, 191, 121, 156, 221, 167, 20, 33, 126, 19, 4, 105, 15, 229, 33, 187, 35, 99, 208, 103, 214, 176, 193, 196, 168, 154, 172, 78, 102, 5, 52, 113, 233, 213, 195, 23, 172, 220, 90, 232, 23, 17, 97, 66, 153, 105, 253, 219, 145, 125, 216, 254, 125, 49, 227, 8, 6, 206, 88, 13],
                vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
                vector[150, 32, 70, 34, 231, 29, 255, 62, 248, 219, 245, 72, 85, 77, 190, 195, 251, 255, 166, 250, 229, 133, 29, 117, 17, 182, 0, 164, 162, 59, 36, 250, 78, 129, 8, 46, 106, 112, 197, 152, 219, 114, 241, 121, 242, 189, 75, 204],
                b"Validator1",
                x"FFFF",
                init_stake,
                option::none(),
                1,
                ctx
            );
            assert!(validator::stake_amount(&validator) == 10, 0);
            assert!(validator::sui_address(&validator) == sender, 0);

            validator::destroy(validator, ctx);
        };

        // Check that after destroy, the original stake still exists.
        test_scenario::next_tx(scenario, sender);
        {
            let stake = test_scenario::take_from_sender<Stake>(scenario);
            assert!(stake::value(&stake) == 10, 0);
            test_scenario::return_to_sender(scenario, stake);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_pending_validator_flow() {
        let sender = @0x8feebb589ffa14667ff721b7cfb186cfad6530fc;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = validator::new(
            sender,
            vector[131, 117, 151, 65, 106, 116, 161, 1, 125, 44, 138, 143, 162, 193, 244, 241, 19, 159, 175, 120, 76, 35, 83, 213, 49, 79, 36, 21, 121, 79, 86, 242, 16, 1, 185, 176, 31, 191, 121, 156, 221, 167, 20, 33, 126, 19, 4, 105, 15, 229, 33, 187, 35, 99, 208, 103, 214, 176, 193, 196, 168, 154, 172, 78, 102, 5, 52, 113, 233, 213, 195, 23, 172, 220, 90, 232, 23, 17, 97, 66, 153, 105, 253, 219, 145, 125, 216, 254, 125, 49, 227, 8, 6, 206, 88, 13],
            vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
            vector[150, 32, 70, 34, 231, 29, 255, 62, 248, 219, 245, 72, 85, 77, 190, 195, 251, 255, 166, 250, 229, 133, 29, 117, 17, 182, 0, 164, 162, 59, 36, 250, 78, 129, 8, 46, 106, 112, 197, 152, 219, 114, 241, 121, 242, 189, 75, 204],
            b"Validator1",
            x"FFFF",
            init_stake,
            option::none(),
            1,
            ctx
        );

        test_scenario::next_tx(scenario, sender);
        {
            let ctx = test_scenario::ctx(scenario);
            let new_stake = coin::into_balance(coin::mint_for_testing(30, ctx));
            validator::request_add_stake(&mut validator, new_stake, option::none(), ctx);

            assert!(validator::stake_amount(&validator) == 10, 0);
            assert!(validator::pending_stake_amount(&validator) == 30, 0);
        };

        test_scenario::next_tx(scenario, sender);
        {
            let stake = test_scenario::take_from_sender<Stake>(scenario);
            let ctx = test_scenario::ctx(scenario);
            validator::request_withdraw_stake(&mut validator, &mut stake, 5, 35, ctx);
            test_scenario::return_to_sender(scenario, stake);
            assert!(validator::stake_amount(&validator) == 10, 0);
            assert!(validator::pending_stake_amount(&validator) == 30, 0);
            assert!(validator::pending_withdraw(&validator) == 5, 0);

            // Calling `adjust_stake_and_gas_price` will withdraw the coin and transfer to sender.
            validator::adjust_stake_and_gas_price(&mut validator);

            assert!(validator::stake_amount(&validator) == 35, 0);
            assert!(validator::pending_stake_amount(&validator) == 0, 0);
            assert!(validator::pending_withdraw(&validator) == 0, 0);
        };

        test_scenario::next_tx(scenario, sender);
        {
            let withdraw = test_scenario::take_from_sender<LockedCoin<SUI>>(scenario);
            assert!(locked_coin::value(&withdraw) == 5, 0);
            test_scenario::return_to_sender(scenario, withdraw);
        };

        validator::destroy(validator, test_scenario::ctx(scenario));
        test_scenario::end(scenario_val);
    }
}
