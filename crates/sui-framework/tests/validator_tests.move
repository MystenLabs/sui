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
        // sender, pubkey and pop are generated via crates/sui-types/src/unit_tests/crypto_tests.rs
        let sender = @0x21b60aa9a8cb189ccbe20461dbfad2202fdef55b;
        let pop = x"89f311605323ce0151b24adba390692e49faa19c9fedad72a04af228796acb3e147cf4b7a85794265bbf6dda15897090";
        let pk = x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
        
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        {
            let ctx = test_scenario::ctx(scenario);

            let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));
            let validator = validator::new(
                sender,
                pk,
                vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
                vector[],
                pop,
                b"Validator1",
                x"FFFF",
                x"FFFF",
                x"FFFF",
                init_stake,
                option::none(),
                1,
                0,
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
        let sender = @0x21b60aa9a8cb189ccbe20461dbfad2202fdef55b;
        let pop = x"89f311605323ce0151b24adba390692e49faa19c9fedad72a04af228796acb3e147cf4b7a85794265bbf6dda15897090";
        let pk = x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";

        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = validator::new(
            sender,
            pk,
            vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38],
            vector[],
            pop,
            b"Validator1",
            x"FFFF",
            x"FFFF",
            x"FFFF",
            init_stake,
            option::none(),
            1,
            0,
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
