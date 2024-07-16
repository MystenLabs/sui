// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module deepbook::master_tests {
    use sui::{
        test_scenario::{
            Scenario,
            begin,
            end,
            return_shared,
            return_to_sender,
        },
        sui::SUI,
        test_utils,
        clock::{Clock},
        coin::Coin,
    };
    use deepbook::{
        balance_manager::{Self, BalanceManager, TradeCap},
        constants,
        pool_tests::{Self},
        pool::{Self, Pool},
        balance_manager_tests::{Self, USDC, SPAM, USDT},
        math,
        balances::{Self, Balances},
        registry::{Self, Registry},
    };
    use token::deep::{Self, DEEP, ProtectedTreasury};

    public struct ExpectedBalances has drop {
        sui: u64,
        usdc: u64,
        spam: u64,
        deep: u64,
        usdt: u64,
    }

    const OWNER: address = @0x1;
    const ALICE: address = @0xAAAA;
    const BOB: address = @0xBBBB;

    const NoError: u64 = 0;
    const EDuplicatePool: u64 = 1;
    const ENotEnoughFunds: u64 = 2;
    const EIncorrectStakeOwner: u64 = 3;
    const ECannotPropose: u64 = 4;
    const EIncorrectRebateClaimer: u64 = 5;
    const EDataRecentlyAdded: u64 = 6;
    const ENoAmountToBurn: u64 = 7;
    const ENoAmountToBurn2: u64 = 8;
    const ENotEnoughBaseForLoan: u64 = 9;
    const ENotEnoughQuoteForLoan: u64 = 10;
    const EIncorrectLoanPool: u64 = 11;
    const EIncorrectTypeReturned: u64 = 12;
    const EInvalidOwner: u64 = 13;
    const ETradeCapNotInList: u64 = 15;
    const EInvalidTrader: u64 = 16;
    const EIncorrectLevel2Price: u64 = 17;
    const EIncorrectLevel2Quantity: u64 = 18;

    #[test]
    fun test_master_ok() {
        test_master(NoError)
    }

    #[test, expected_failure(abort_code = ::deepbook::registry::EPoolAlreadyExists)]
    fun test_master_duplicate_pool_e() {
        test_master(EDuplicatePool)
    }

    #[test, expected_failure(abort_code = ::deepbook::balance_manager::EBalanceManagerBalanceTooLow)]
    fun test_master_not_enough_funds_e() {
        test_master(ENotEnoughFunds)
    }

    #[test, expected_failure(abort_code = ::deepbook::balance_manager::EInvalidOwner)]
    fun test_master_incorrect_stake_owner_e() {
        test_master(EIncorrectStakeOwner)
    }

    #[test, expected_failure(abort_code = ::deepbook::state::ENoStake)]
    fun test_master_cannot_propose_e() {
        test_master(ECannotPropose)
    }

    #[test, expected_failure(abort_code = ::deepbook::balance_manager::EInvalidTrader)]
    fun test_master_incorrect_rebate_claimer_e() {
        test_master(EIncorrectRebateClaimer)
    }

    #[test, expected_failure(abort_code = ::deepbook::pool::ENoAmountToBurn)]
    fun test_no_amount_to_burn_e() {
        test_master(ENoAmountToBurn)
    }

    #[test, expected_failure(abort_code = ::deepbook::pool::ENoAmountToBurn)]
    fun test_no_amount_to_burn_2_e() {
        test_master(ENoAmountToBurn2)
    }

    #[test]
    fun test_master_deep_price_ok() {
        test_master_deep_price(NoError)
    }

    #[test, expected_failure(abort_code = ::deepbook::deep_price::EDataPointRecentlyAdded)]
    fun test_master_deep_price_recently_added_e() {
        test_master_deep_price(EDataRecentlyAdded)
    }

    #[test]
    fun test_master_update_treasury_address_ok() {
        test_master_update_treasury_address()
    }

    #[test]
    fun test_master_both_conversion_available_ok() {
        test_master_both_conversion_available()
    }

    #[test]
    fun test_flash_loan_ok() {
        test_flash_loan(NoError)
    }

    #[test, expected_failure(abort_code = ::deepbook::vault::ENotEnoughBaseForLoan)]
    fun test_flash_loan_base_e() {
        test_flash_loan(ENotEnoughBaseForLoan)
    }

    #[test, expected_failure(abort_code = ::deepbook::vault::ENotEnoughQuoteForLoan)]
    fun test_flash_loan_quote_e() {
        test_flash_loan(ENotEnoughQuoteForLoan)
    }

    #[test, expected_failure(abort_code = ::deepbook::vault::EIncorrectLoanPool)]
    fun test_flash_loan_incorrect_pool_e() {
        test_flash_loan(EIncorrectLoanPool)
    }

    #[test, expected_failure(abort_code = ::deepbook::vault::EIncorrectTypeReturned)]
    fun test_flash_loan_incorrect_type_returned_e() {
        test_flash_loan(EIncorrectTypeReturned)
    }

    #[test]
    fun test_trader_permission_and_modify_returned_ok(){
        test_trader_permission_and_modify_returned(NoError)
    }

    #[test, expected_failure(abort_code = ::deepbook::balance_manager::EInvalidOwner)]
    fun test_trader_permission_and_modify_returned_invalid_owner_e(){
        test_trader_permission_and_modify_returned(EInvalidOwner)
    }

    #[test, expected_failure(abort_code = ::deepbook::balance_manager::ETradeCapNotInList)]
    fun test_trader_permission_and_modify_trader_not_in_list_e(){
        test_trader_permission_and_modify_returned(ETradeCapNotInList)
    }

    #[test, expected_failure(abort_code = ::deepbook::balance_manager::EInvalidTrader)]
    fun test_trader_permission_invalid_trader_e() {
        test_trader_permission_and_modify_returned(EInvalidTrader)
    }

    #[test]
    fun test_get_level_2_range_ok(){
        test_get_level_2_range()
    }

    // === Test Functions ===
    fun test_master(
        error_code: u64,
    ) {
        let mut test = begin(OWNER);
        let registry_id = pool_tests::setup_test(OWNER, &mut test);
        pool_tests::set_time(0, &mut test);

        let starting_balance = 10000 * constants::float_scaling();
        let owner_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            OWNER,
            starting_balance,
            &mut test
        );

        // Create two pools, one with SUI as base asset and one with SPAM as base asset
        let pool1_reference_id = pool_tests::setup_reference_pool<SUI, DEEP>(OWNER, registry_id, owner_balance_manager_id, 100 * constants::float_scaling(), &mut test);
        let pool2_reference_id = pool_tests::setup_reference_pool<SPAM, DEEP>(OWNER, registry_id, owner_balance_manager_id, 100 * constants::float_scaling(), &mut test);

        // Create two pools, one with SUI as base asset and one with SPAM as base asset
        let pool1_id = pool_tests::setup_pool_with_default_fees<SUI, USDC>(OWNER, registry_id, false, &mut test);
        if (error_code == EDuplicatePool) {
            pool_tests::setup_pool_with_default_fees<USDC, SUI>(OWNER, registry_id, false, &mut test);
        };
        let pool2_id = pool_tests::setup_pool_with_default_fees<SPAM, USDC>(OWNER, registry_id, false, &mut test);

        // Default price point of 100 deep per base will be added
        pool_tests::add_deep_price_point<SUI, USDC, SUI, DEEP>(
            OWNER,
            pool1_id,
            pool1_reference_id,
            &mut test,
        );
        pool_tests::add_deep_price_point<SPAM, USDC, SPAM, DEEP>(
            OWNER,
            pool2_id,
            pool2_reference_id,
            &mut test,
        );

        let alice_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            ALICE,
            starting_balance,
            &mut test
        );
        let bob_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            BOB,
            starting_balance,
            &mut test
        );

        // variables to input into order
        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let price = 2 * constants::float_scaling();
        let quantity = 3 * constants::float_scaling();
        let big_quantity = 1_000_000 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let is_bid = true;
        let pay_with_deep = true;
        let mut maker_fee = constants::maker_fee();
        let taker_fee;
        let deep_multiplier = constants::deep_multiplier();
        let mut alice_balance = ExpectedBalances{
            sui: starting_balance,
            usdc: starting_balance,
            spam: starting_balance,
            deep: starting_balance,
            usdt: starting_balance,
        };
        let mut bob_balance = ExpectedBalances{
            sui: starting_balance,
            usdc: starting_balance,
            spam: starting_balance,
            deep: starting_balance,
            usdt: starting_balance,
        };

        // Epoch 0
        assert!(test.ctx().epoch() == 0, 0);

        if (error_code == ENotEnoughFunds) {
            pool_tests::place_limit_order<SUI, USDC>(
                ALICE,
                pool1_id,
                alice_balance_manager_id,
                client_order_id,
                order_type,
                constants::self_matching_allowed(),
                price,
                big_quantity,
                is_bid,
                pay_with_deep,
                expire_timestamp,
                &mut test,
            );
        };

        // Alice places bid order in pool 1
        let order_info_1 = pool_tests::place_limit_order<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );
        alice_balance.usdc = alice_balance.usdc - math::mul(price, quantity);
        alice_balance.deep = alice_balance.deep - math::mul(
            math::mul(maker_fee, deep_multiplier),
            quantity
        );

        // Alice places ask order in pool 2
        pool_tests::place_limit_order<SPAM, USDC>(
            ALICE,
            pool2_id,
            alice_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        alice_balance.spam = alice_balance.spam - quantity;
        alice_balance.deep = alice_balance.deep - math::mul(
            math::mul(maker_fee, deep_multiplier),
            quantity
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Alice stakes 100 DEEP into pool 1 during epoch 0 to be effective in epoch 1
        stake<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            200 * constants::float_scaling(),
            &mut test
        );
        alice_balance.deep = alice_balance.deep - 200 * constants::float_scaling();
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        if (error_code == EIncorrectStakeOwner) {
            stake<SUI, USDC>(
                BOB,
                pool1_id,
                alice_balance_manager_id,
                200 * constants::float_scaling(),
                &mut test
            );
        };

        // Bob stakes 100 DEEP into pool 1 during epoch 1
        stake<SUI, USDC>(
            BOB,
            pool1_id,
            bob_balance_manager_id,
            100 * constants::float_scaling(),
            &mut test
        );
        bob_balance.deep = bob_balance.deep - 100 * constants::float_scaling();
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        if (error_code == ECannotPropose) {
            submit_proposal<SUI, USDC>(
                ALICE,
                pool1_id,
                alice_balance_manager_id,
                600_000,
                200_000,
                100 * constants::float_scaling(),
                &mut test
            );
        };

        // Epoch 1
        // Alice now has a stake of 100 that's effective
        // Alice proposes a change to the maker fee for epoch 2
        // Governance changed maker fees to 0.02%, taker fees to 0.06%, same deep staking required
        test.next_epoch(OWNER);
        assert!(test.ctx().epoch() == 1, 0);

        submit_proposal<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            600_000,
            200_000,
            100 * constants::float_scaling(),
            &mut test
        );

        // Epoch 2 (Trades happen this epoch)
        // New trading fees are in effect for pool 1
        // Stakes are in effect for both Alice and Bob
        test.next_epoch(OWNER);
        assert!(test.ctx().epoch() == 2, 0);
        let old_maker_fee = maker_fee;
        maker_fee = 200_000;
        taker_fee = 600_000;

        // Alice should get refunded the previous fees for the order
        pool_tests::cancel_order<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            order_info_1.order_id(),
            &mut test
        );
        alice_balance.usdc = alice_balance.usdc + math::mul(price, quantity);
        alice_balance.deep = alice_balance.deep + math::mul(
            math::mul(old_maker_fee, deep_multiplier),
            quantity
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        let client_order_id = 2;

        // Alice should pay new fees for the order, maker fee should be 0.02%
        pool_tests::place_limit_order<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );
        alice_balance.usdc = alice_balance.usdc - math::mul(price, quantity);
        alice_balance.deep = alice_balance.deep - math::mul(
            math::mul(maker_fee, deep_multiplier),
            quantity
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        let executed_quantity = 3 * constants::float_scaling();
        let quantity = 100 * constants::float_scaling();

        // Bob places market ask order with large size in pool 1, only quantity 3 should be filled with Alice's bid order
        // Bob will not get discounted fees as even though he's staked, there's volume traded yet
        // Taker fee paid should be 0.06%
        pool_tests::place_market_order<SUI, USDC>(
            BOB,
            pool1_id,
            bob_balance_manager_id,
            client_order_id,
            constants::self_matching_allowed(),
            quantity,
            !is_bid,
            pay_with_deep,
            &mut test,
        );
        bob_balance.sui = bob_balance.sui - executed_quantity;
        bob_balance.usdc = bob_balance.usdc + math::mul(price, executed_quantity);
        bob_balance.deep = bob_balance.deep - math::mul(
            math::mul(taker_fee, deep_multiplier),
            executed_quantity
        );
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        // Alice withdraws settled amounts twice, should only settle once
        withdraw_settled_amounts<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            &mut test
        );
        alice_balance.sui = alice_balance.sui + executed_quantity;

        withdraw_settled_amounts<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            &mut test
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Epoch 3, Alice proposes new fees, then unstakes
        // Bob proposes new fees as well after Alice unstakes, but quorum is based on old voting power
        // So neither proposal is passed
        // Stake of 200 deep should be returned to Alice, new proposal not passed
        test.next_epoch(OWNER);
        assert!(test.ctx().epoch() == 3, 0);

        submit_proposal<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            800_000,
            400_000,
            100 * constants::float_scaling(),
            &mut test
        );

        unstake<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            &mut test
        );
        alice_balance.deep = alice_balance.deep + 200 * constants::float_scaling();
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        submit_proposal<SUI, USDC>(
            BOB,
            pool1_id,
            bob_balance_manager_id,
            900_000,
            500_000,
            100 * constants::float_scaling(),
            &mut test
        );

        // Epoch 4
        // Alice earned the 0.08% total fee collected in epoch 2
        // Alice 0.02% maker fee + Bob 0.06% taker = 0.08% total fees
        // Alice will make a claim for the fees collected
        // Bob will get no rebates as he only executed taker orders
        test.next_epoch(OWNER);
        assert!(test.ctx().epoch() == 4, 0);

        claim_rebates<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            &mut test
        );
        let quantity = 3 * constants::float_scaling();
        alice_balance.deep = alice_balance.deep + math::mul(
            quantity,
            math::mul(800_000, deep_multiplier)
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        if (error_code == EIncorrectRebateClaimer) {
            claim_rebates<SUI, USDC>(
                BOB,
                pool1_id,
                alice_balance_manager_id,
                &mut test
            );
        };

        // Bob will get no rebates
        claim_rebates<SUI, USDC>(
            BOB,
            pool1_id,
            bob_balance_manager_id,
            &mut test
        );
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        // Alice restakes 100 DEEP into pool 1 during epoch 4
        stake<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            100 * constants::float_scaling(),
            &mut test
        );
        alice_balance.deep = alice_balance.deep - 100 * constants::float_scaling();
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Now the vault balance should only include the 2 stakes of 100 DEEP
        check_vault_balances<SUI, USDC>(
            pool1_id,
            &balances::new(
                0,
                0,
                200 * constants::float_scaling()
            ),
            &mut test
        );

        // Advance to epoch 28
        let quantity = 1 * constants::float_scaling();
        let mut i = 23;
        // For 23 epochs, Alice and Bob will both make 1 quantity per epoch, and should get the full rebate
        // Alice will place a bid for quantity 1, bob will place ask for quantity 2, then alice will place a bid for quantity 1
        // Discount will be 50% for Alice because sheplaces a maker order first that's taken
        // Bob will not get discount because he takes Alice's order
        // Fees paid for each should be 0.02% maker for both, 0.03% taker for Alice, 0.06% taker for Bob
        // Total fees collected should be 0.065% for each epoch
        // Alice should have 46 more SUI at the end of the loop
        // Bob should have 92 more USDC at the end of the loop
        while (i > 0) {
            test.next_epoch(OWNER);
            execute_cross_trading<SUI, USDC>(
                pool1_id,
                alice_balance_manager_id,
                bob_balance_manager_id,
                client_order_id,
                order_type,
                price,
                quantity,
                is_bid,
                pay_with_deep,
                constants::max_u64(),
                &mut test
            );
            i = i - 1;
        };
        let taker_sui_traded = 23 * constants::float_scaling();
        let maker_sui_traded = 23 * constants::float_scaling();
        let quantity_sui_traded = taker_sui_traded + maker_sui_traded;
        let avg_taker_fee = math::mul(taker_fee + math::mul(constants::half(), taker_fee), constants::half());
        alice_balance.sui = alice_balance.sui + quantity_sui_traded;
        alice_balance.usdc = alice_balance.usdc - math::mul(price, quantity_sui_traded);
        alice_balance.deep = alice_balance.deep - math::mul(
            math::mul(taker_sui_traded, math::mul(constants::half(), taker_fee)) + math::mul(maker_sui_traded, maker_fee),
            deep_multiplier
        );
        bob_balance.sui = bob_balance.sui - quantity_sui_traded;
        bob_balance.usdc = bob_balance.usdc + math::mul(price, quantity_sui_traded);
        bob_balance.deep = bob_balance.deep - math::mul(
            math::mul(taker_sui_traded, taker_fee) + math::mul(maker_sui_traded, maker_fee),
            deep_multiplier
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        test.next_epoch(OWNER);
        assert!(test.ctx().epoch() == 28, 0);

        // Alice claims rebates for the past 23 epochs
        claim_rebates<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            &mut test
        );
        let alice_rebates = math::mul(
            math::mul(taker_sui_traded, avg_taker_fee) + math::mul(maker_sui_traded, maker_fee),
            deep_multiplier
        );
        alice_balance.deep = alice_balance.deep + alice_rebates;
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Bob claims rebates for the past 23 epochs
        claim_rebates<SUI, USDC>(
            BOB,
            pool1_id,
            bob_balance_manager_id,
            &mut test
        );
        let bob_rebates = math::mul(
            math::mul(taker_sui_traded, avg_taker_fee) + math::mul(maker_sui_traded, maker_fee),
            deep_multiplier
        );
        bob_balance.deep = bob_balance.deep + bob_rebates;
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        // Since all rebates are to be claimed, there are no amounts to burn
        if (error_code == ENoAmountToBurn) {
            burn_deep<SUI, USDC>(
                ALICE,
                pool1_id,
                0,
                &mut test
            );
        };

        // Same cross trading happens during epoch 28
        // quantity being traded is halved, each person will make 0.5 quantity and take 0.5 quantity
        let quantity = 500_000_000;
        execute_cross_trading<SUI, USDC>(
            pool1_id,
            alice_balance_manager_id,
            bob_balance_manager_id,
            client_order_id,
            order_type,
            price,
            quantity,
            is_bid,
            pay_with_deep,
            constants::max_u64(),
            &mut test
        );
        let taker_sui_traded = quantity;
        let maker_sui_traded = quantity;
        let quantity_sui_traded = taker_sui_traded + maker_sui_traded;
        alice_balance.sui = alice_balance.sui + quantity_sui_traded;
        alice_balance.usdc = alice_balance.usdc - math::mul(price, quantity_sui_traded);
        alice_balance.deep = alice_balance.deep - math::mul(
            math::mul(taker_sui_traded, taker_fee) + math::mul(maker_sui_traded, maker_fee),
            deep_multiplier
        );
        bob_balance.sui = bob_balance.sui - quantity_sui_traded;
        bob_balance.usdc = bob_balance.usdc + math::mul(price, quantity_sui_traded);
        bob_balance.deep = bob_balance.deep - math::mul(
            math::mul(taker_sui_traded, taker_fee) + math::mul(maker_sui_traded, maker_fee),
            deep_multiplier
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        // Epoch 29. Rebates should now be using the normal calculation
        test.next_epoch(OWNER);
        assert!(test.ctx().epoch() == 29, 0);
        claim_rebates<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            &mut test
        );
        claim_rebates<SUI, USDC>(
            BOB,
            pool1_id,
            bob_balance_manager_id,
            &mut test
        );
        let fees_generated = math::mul(
            2 * (math::mul(taker_sui_traded, taker_fee) + math::mul(maker_sui_traded, maker_fee)),
            deep_multiplier
        );
        let historic_median = 2 * constants::float_scaling();
        let other_maker_liquidity = 500_000_000;
        let maker_rebate_percentage = if (historic_median > 0) {
            constants::float_scaling() - math::min(constants::float_scaling(), math::div(other_maker_liquidity, historic_median))
        } else {
            0
        }; // 75%

        let maker_volume_proportion = 500_000_000;
        let maker_fee_proportion = math::mul(maker_volume_proportion, fees_generated); // 4000000
        let maker_rebate = math::mul(maker_rebate_percentage, maker_fee_proportion); // 3000000
        let expected_amount_burned = fees_generated - 2 * maker_rebate; // 1000000
        alice_balance.deep = alice_balance.deep + maker_rebate;
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );
        bob_balance.deep = bob_balance.deep + maker_rebate;
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );
        burn_deep<SUI, USDC>(
            ALICE,
            pool1_id,
            expected_amount_burned,
            &mut test
        );

        // Trying to burn again will fail
        if (error_code == ENoAmountToBurn2) {
            burn_deep<SUI, USDC>(
                ALICE,
                pool1_id,
                0,
                &mut test
            );
        };

        end(test);
    }

    fun test_master_deep_price(
        error_code: u64,
    ){
        let mut test = begin(OWNER);
        let registry_id = pool_tests::setup_test(OWNER, &mut test);

        let starting_balance = 10000 * constants::float_scaling();

        let owner_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            OWNER,
            starting_balance,
            &mut test
        );

        // Create two pools, pool 1 will be used as reference pool
        let pool1_id = pool_tests::setup_reference_pool<SUI, DEEP>(OWNER, registry_id, owner_balance_manager_id, 100 * constants::float_scaling(), &mut test);
        let pool2_id = pool_tests::setup_pool_with_default_fees<SPAM, SUI>(OWNER, registry_id, false, &mut test);

        // Default price point of 10 deep per base (SPAM) will be added
        pool_tests::set_time(0, &mut test);
        pool_tests::add_deep_price_point<SPAM, SUI, SUI, DEEP>(
            OWNER,
            pool2_id,
            pool1_id,
            &mut test,
        );

        // Default mid price for pool should be 100 for tests only
        check_mid_price<SUI, DEEP>(
            pool1_id,
            100 * constants::float_scaling(),
            &mut test
        );

        let alice_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            ALICE,
            starting_balance,
            &mut test
        );
        let bob_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            BOB,
            starting_balance,
            &mut test
        );

        // variables to input into order
        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let price = 100 * constants::float_scaling();
        let quantity = 2 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let is_bid = true;
        let pay_with_deep = true;
        let maker_fee = constants::maker_fee();
        let taker_fee = constants::taker_fee();
        let mut alice_balance = ExpectedBalances{
            sui: starting_balance,
            usdc: starting_balance,
            spam: starting_balance,
            deep: starting_balance,
            usdt: starting_balance,
        };
        let mut bob_balance = ExpectedBalances{
            sui: starting_balance,
            usdc: starting_balance,
            spam: starting_balance,
            deep: starting_balance,
            usdt: starting_balance,
        };

        // Epoch 0. Pool 1 is whitelisted, which means 0 trading fees.
        assert!(test.ctx().epoch() == 0, 0);

        // Trading within pool 1 should have no fees
        // Alice should get 2 more sui, Bob should lose 2 sui
        // Alice should get 200 less deep, Bob should get 200 deep
        execute_cross_trading<SUI, DEEP>(
            pool1_id,
            alice_balance_manager_id,
            bob_balance_manager_id,
            client_order_id,
            order_type,
            price, // 100 * constants::float_scaling();
            quantity, // 1 * constants::float_scaling();
            is_bid,
            pay_with_deep,
            constants::max_u64(),
            &mut test
        );
        alice_balance.sui = alice_balance.sui + 2 * quantity;
        alice_balance.deep = alice_balance.deep - 2 * math::mul(price, quantity);
        bob_balance.sui = bob_balance.sui - 2 * quantity;
        bob_balance.deep = bob_balance.deep + 2 * math::mul(price, quantity);
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        // Alice stakes 100 deep into pool 2
        stake<SPAM, SUI>(
            ALICE,
            pool2_id,
            alice_balance_manager_id,
            100 * constants::float_scaling(),
            &mut test
        );
        alice_balance.deep = alice_balance.deep - 100 * constants::float_scaling();

        pool_tests::set_time(100_000, &mut test);

        // Bob stakes 100 deep into pool 2
        stake<SPAM, SUI>(
            BOB,
            pool2_id,
            bob_balance_manager_id,
            100 * constants::float_scaling(),
            &mut test
        );
        bob_balance.deep = bob_balance.deep - 100 * constants::float_scaling();

        // Alice places a bid order in pool 1 with quantity 1, price 125
        let price = 125 * constants::float_scaling();
        pool_tests::place_limit_order<SUI, DEEP>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );
        // Alice should have 125 less deep
        alice_balance.deep = alice_balance.deep - math::mul(price, quantity);

        // Bob places a ask order in pool 1 with quantity 1, price 175
        let price = 175 * constants::float_scaling();
        pool_tests::place_limit_order<SUI, DEEP>(
            BOB,
            pool1_id,
            bob_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        // Bob should have 1 less sui
        bob_balance.sui = bob_balance.sui - quantity;
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        // Epoch 1
        // Pool 2 now uses pool 1 as reference pool
        // Pool 2 deep per base should be 0 (non functional)
        // Pool 2 deep per quote should be 150 (150 DEEP per SUI)
        // Stakes in pool 2 for both Alice and Bob are in effect
        test.next_epoch(OWNER);
        assert!(test.ctx().epoch() == 1, 0);
        pool_tests::set_time(200_000, &mut test);

        // New deep per quote is 125, the average of 100 and 150.
        pool_tests::add_deep_price_point<SPAM, SUI, SUI, DEEP>(
            OWNER,
            pool2_id,
            pool1_id,
            &mut test
        );

        // Check data added is 150
        check_mid_price<SUI, DEEP>(
            pool1_id,
            150 * constants::float_scaling(),
            &mut test
        );

        // Cannot add deep price point again because it was added too recently
        if (error_code == EDataRecentlyAdded) {
            pool_tests::add_deep_price_point<SPAM, SUI, SUI, DEEP>(
                OWNER,
                pool2_id,
                pool1_id,
                &mut test
            );
        };

        let price = 10 * constants::float_scaling();
        // Alice places a bid order in pool 2 with quantity 1, price 10
        // Maker fee should be the default of 0.05%
        // Since deep per sui is 150 (based on orders placed in pool 1),
        // Alice should pay 0.05% * (quantity 1 * price 10 * conversion 150) = 0.75 DEEP
        // Alice also pays 10 SUI for SPAM
        let order_info = pool_tests::place_limit_order<SPAM, SUI>(
            ALICE,
            pool2_id,
            alice_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );
        let deep_multiplier = 125_000_000_000;
        alice_balance.sui = alice_balance.sui - math::mul(price, quantity);
        alice_balance.deep = alice_balance.deep - math::mul(
            math::mul(maker_fee, math::mul(price, quantity)),
            deep_multiplier
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Alice cancels the order, gets the exact same refund back
        pool_tests::cancel_order<SPAM, SUI>(
            ALICE,
            pool2_id,
            alice_balance_manager_id,
            order_info.order_id(),
            &mut test
        );
        alice_balance.sui = alice_balance.sui + math::mul(price, quantity);
        alice_balance.deep = alice_balance.deep + math::mul(
            math::mul(maker_fee, math::mul(price, quantity)),
            deep_multiplier
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        let price = 10 * constants::float_scaling();
        let quantity = 3 * constants::float_scaling();
        // Alice places a bid order in pool 2 with quantity 1, price 10
        // Bob will take and place an additional quantity 1, price 10 order
        // Then Alice will take
        // Both alice and bob will pay 0.05% * (quantity 1 * price 10 * conversion 150) = 0.75 DEEP
        // for maker order and 0.10% * (quantity 1 * price 10 * conversion 150) = 0.75 DEEP for taker order
        execute_cross_trading<SPAM, SUI>(
            pool2_id,
            alice_balance_manager_id,
            bob_balance_manager_id,
            client_order_id,
            order_type,
            price,
            quantity,
            is_bid,
            pay_with_deep,
            constants::max_u64(),
            &mut test
        );
        let maker_quantity_traded = quantity;
        let taker_quantity_traded = quantity;
        let quantity_traded = maker_quantity_traded + taker_quantity_traded;
        let alice_maker_fee = math::mul(
            math::mul(maker_fee, math::mul(price, quantity)),
            deep_multiplier
        );
        let alice_taker_fee = math::mul(
            math::mul(math::mul(constants::half(), taker_fee), math::mul(price, quantity)),
            deep_multiplier
        );
        alice_balance.spam = alice_balance.spam + quantity_traded;
        alice_balance.sui = alice_balance.sui - math::mul(price, quantity_traded);

        let bob_maker_fee = math::mul(
            math::mul(maker_fee, math::mul(price, quantity)),
            deep_multiplier
        );
        let bob_taker_fee = math::mul(
            math::mul(taker_fee, math::mul(price, quantity)),
            deep_multiplier
        );
        alice_balance.deep = alice_balance.deep - alice_maker_fee - alice_taker_fee;
        bob_balance.spam = bob_balance.spam - quantity_traded;
        bob_balance.sui = bob_balance.sui + math::mul(price, quantity_traded);
        bob_balance.deep = bob_balance.deep - bob_maker_fee - bob_taker_fee;

        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        end(test);
    }

    fun test_flash_loan(
        error_code: u64,
    ){
        let mut test = begin(OWNER);
        let registry_id = pool_tests::setup_test(OWNER, &mut test);
        pool_tests::set_time(0, &mut test);

        let starting_balance = 10000 * constants::float_scaling();
        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let expire_timestamp = constants::max_u64();
        let is_bid = true;
        let pay_with_deep = true;
        let taker_fee = constants::taker_fee();

        let owner_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            OWNER,
            starting_balance,
            &mut test
        );

        let alice_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            ALICE,
            starting_balance,
            &mut test
        );

        let mut alice_balance = ExpectedBalances{
            sui: starting_balance,
            usdc: starting_balance,
            spam: starting_balance,
            deep: starting_balance,
            usdt: starting_balance,
        };

        // Create the DEEP reference pool SUI/DEEP
        let reference_pool_id = pool_tests::setup_reference_pool<SUI, DEEP>(OWNER, registry_id, owner_balance_manager_id, 100 * constants::float_scaling(), &mut test);
        // Create the SUI/USDT pool
        let pool_id = pool_tests::setup_pool_with_default_fees<SUI, USDT>(OWNER, registry_id, false, &mut test);

        // Alice now has no DEEP and SUI after withdrawal and burn for testing
        withdraw_and_burn<DEEP>(
            ALICE,
            alice_balance_manager_id,
            10000 * constants::float_scaling(),
            &mut test
        );
        withdraw_and_burn<SUI>(
            ALICE,
            alice_balance_manager_id,
            10000 * constants::float_scaling(),
            &mut test
        );
        alice_balance.deep = 0;
        alice_balance.sui = 0;
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Owner adds a price point of default 100 DEEP per SUI to the SUI/USDT pool
        pool_tests::add_deep_price_point<SUI, USDT, SUI, DEEP>(
            OWNER,
            pool_id,
            reference_pool_id,
            &mut test,
        );

        let price = 100 * constants::float_scaling();
        let quantity = 10 * constants::float_scaling();

        // Owner places a bid order of at price 100 for 10 SUI into pool 1, which is a SUI/DEEP pool
        // This allows for flash loans
        pool_tests::place_limit_order<SUI, DEEP>(
            OWNER,
            reference_pool_id,
            owner_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        // Owner places a sell order in the SUI/USDC pool
        let price = 2 * constants::float_scaling();
        let quantity = 10 * constants::float_scaling();

        pool_tests::place_limit_order<SUI, USDT>(
            OWNER,
            pool_id,
            owner_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        test.next_tx(ALICE);
        {
            let mut loan_pool = test.take_shared_by_id<Pool<SUI, DEEP>>(reference_pool_id);
            let mut target_pool = test.take_shared_by_id<Pool<SUI, USDT>>(pool_id);
            let clock = test.take_shared<Clock>();
            let mut alice_balance_manager = test.take_shared_by_id<BalanceManager>(alice_balance_manager_id);
            let trade_proof = alice_balance_manager.generate_proof_as_owner(test.ctx());

            // Alice wants to swap 10 USDT for 5 SUI in the SUI/USDT pool, but has no SUI to swap to DEEP
            // Alice will borrow 100 DEEP from the SUI/DEEP pool
            // If Alice tries to borrow too much from the pool, there will be an error

            let quote_needed = if (error_code == ENotEnoughQuoteForLoan) {
                10000 * constants::float_scaling()
            } else {
                100 * constants::float_scaling()
            };
            if (error_code == ENotEnoughBaseForLoan) {
                let base_needed = 10000 * constants::float_scaling();
                let (_base_borrowed, _flash_loan) = pool::borrow_flashloan_base<SUI, DEEP>(
                    &mut loan_pool,
                    base_needed,
                    test.ctx(),
                );
                abort 0
            };

            let (quote_borrowed, flash_loan) = pool::borrow_flashloan_quote<SUI, DEEP>(
                &mut loan_pool,
                quote_needed,
                test.ctx(),
            );
            alice_balance.deep = alice_balance.deep + quote_needed;

            assert!(quote_borrowed.value() == quote_needed, 0);

            // Alice deposits the 100 DEEP into her balance_manager
            alice_balance_manager.deposit<DEEP>(quote_borrowed, test.ctx());

            // Alice places a bid order of 5 SUI at price 2, pays fees in DEEP
            // This will match with owner's sell order
            let price = 2 * constants::float_scaling();
            let quantity = 5 * constants::float_scaling();
            target_pool.place_limit_order<SUI, USDT>(
                &mut alice_balance_manager,
                &trade_proof,
                client_order_id,
                order_type,
                constants::self_matching_allowed(),
                price,
                quantity,
                is_bid,
                pay_with_deep,
                expire_timestamp,
                &clock,
                test.ctx(),
            );

            // Alice should now have 5 more SUI (originally at 0) and 10 less USDT
            // Alice should traded 5 SUI, which is 500 in DEEP quantity,
            // since taker fee is 0.10%, Alice should pay 0.10% * 500 = 0.5 DEEP
            alice_balance.sui = alice_balance.sui + quantity;
            alice_balance.usdt = alice_balance.usdt - math::mul(quantity, price);
            alice_balance.deep = alice_balance.deep - math::mul(
                math::mul(taker_fee, quantity),
                constants::deep_multiplier()
            );

            let quantity = 1 * constants::float_scaling();
            let price = 100 * constants::float_scaling();

            // Alice needs to swap SUI back to DEEP in a deep pool, in this scenario also the loan pool
            // to pay back the flash loan. She places a market order of 1 SUI for 100 DEEP.
            // Alice is matched with owner's price of 100 in reference pool
            loan_pool.place_market_order<SUI, DEEP>(
                &mut alice_balance_manager,
                &trade_proof,
                client_order_id,
                constants::self_matching_allowed(),
                quantity,
                !is_bid,
                pay_with_deep,
                &clock,
                test.ctx(),
            );
            alice_balance.sui = alice_balance.sui - quantity;
            alice_balance.deep = alice_balance.deep + math::mul(quantity, price);

            // Alice withdraws the 1 DEEP she borrowed from balance_manager and returns the loan
            let quote_return = alice_balance_manager.withdraw<DEEP>(quote_needed, test.ctx());

            if (error_code == EIncorrectLoanPool) {
                let wrong_quote_return = alice_balance_manager.withdraw<USDT>(quote_needed, test.ctx());
                target_pool.return_flashloan_quote(wrong_quote_return, flash_loan);
                abort 0
            };
            loan_pool.return_flashloan_quote(quote_return, flash_loan);
            alice_balance.deep = alice_balance.deep - quote_needed;

            return_shared(alice_balance_manager);
            return_shared(clock);
            return_shared(target_pool);
            return_shared(loan_pool);
        };

        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Alice borrows and returns the base asset, deposits into manager,
        // withdraws and returns the base asset
        // No balance changes
        test.next_tx(ALICE);
        {
            let mut loan_pool = test.take_shared_by_id<Pool<SUI, DEEP>>(reference_pool_id);
            let clock = test.take_shared<Clock>();
            let mut alice_balance_manager = test.take_shared_by_id<BalanceManager>(alice_balance_manager_id);

            let base_needed = 1 * constants::float_scaling();
            let (base_borrowed, flash_loan) = pool::borrow_flashloan_base<SUI, DEEP>(
                &mut loan_pool,
                base_needed,
                test.ctx(),
            );
            alice_balance.deep = alice_balance.deep + base_needed;

            assert!(base_borrowed.value() == base_needed, 0);

            alice_balance_manager.deposit<SUI>(base_borrowed, test.ctx());

            let base_return = alice_balance_manager.withdraw<SUI>(base_needed, test.ctx());
            loan_pool.return_flashloan_base(base_return, flash_loan);
            alice_balance.deep = alice_balance.deep - base_needed;

            return_shared(alice_balance_manager);
            return_shared(clock);
            return_shared(loan_pool);
        };

        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Alice borrows the base asset, and tries to return the quote asset in place of the base asset
        // This will fail
        if (error_code == EIncorrectTypeReturned){
            test.next_tx(ALICE);
            {
                let mut loan_pool = test.take_shared_by_id<Pool<SUI, DEEP>>(reference_pool_id);
                let clock = test.take_shared<Clock>();
                let mut alice_balance_manager = test.take_shared_by_id<BalanceManager>(alice_balance_manager_id);

                let base_needed = 1 * constants::float_scaling();
                let (base_borrowed, flash_loan) = pool::borrow_flashloan_base<SUI, DEEP>(
                    &mut loan_pool,
                    base_needed,
                    test.ctx(),
                );
                alice_balance.deep = alice_balance.deep + base_needed;

                assert!(base_borrowed.value() == base_needed, 0);

                alice_balance_manager.deposit<SUI>(base_borrowed, test.ctx());

                let quote_return = alice_balance_manager.withdraw<DEEP>(base_needed, test.ctx());
                loan_pool.return_flashloan_quote(quote_return, flash_loan);
                alice_balance.deep = alice_balance.deep - base_needed;

                return_shared(alice_balance_manager);
                return_shared(clock);
                return_shared(loan_pool);
            };
        };

        end(test);
    }

    // Test when there are 2 reference pools, and price points are added to both, the quote conversion is used
    fun test_master_both_conversion_available(){
        let mut test = begin(OWNER);
        let registry_id = pool_tests::setup_test(OWNER, &mut test);
        pool_tests::set_time(0, &mut test);

        let starting_balance = 10000 * constants::float_scaling();
        let owner_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            OWNER,
            starting_balance,
            &mut test
        );

        // Create two pools, one with SUI as base asset and one with SPAM as base asset
        // Conversion is 100 DEEP per SUI, 95 DEEP per SPAM
        let pool1_reference_id = pool_tests::setup_reference_pool<SUI, DEEP>(OWNER, registry_id, owner_balance_manager_id, 100 * constants::float_scaling(), &mut test);
        let pool2_reference_id = pool_tests::setup_reference_pool<SPAM, DEEP>(OWNER, registry_id, owner_balance_manager_id, 95 * constants::float_scaling(), &mut test);

        // Create two pools, one with SUI as base asset and one with SPAM as base asset
        let pool1_id = pool_tests::setup_pool_with_default_fees<SUI, SPAM>(OWNER, registry_id, false, &mut test);

        // Conversion is 100 DEEP per SUI, 95 DEEP per SPAM
        pool_tests::add_deep_price_point<SUI, SPAM, SUI, DEEP>(
            OWNER,
            pool1_id,
            pool1_reference_id,
            &mut test,
        );
        pool_tests::add_deep_price_point<SUI, SPAM, SPAM, DEEP>(
            OWNER,
            pool1_id,
            pool2_reference_id,
            &mut test,
        );

        let alice_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            ALICE,
            starting_balance,
            &mut test
        );
        let bob_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            BOB,
            starting_balance,
            &mut test
        );
        let mut alice_balance = ExpectedBalances{
            sui: starting_balance,
            usdc: starting_balance,
            spam: starting_balance,
            deep: starting_balance,
            usdt: starting_balance,
        };
        let mut bob_balance = ExpectedBalances{
            sui: starting_balance,
            usdc: starting_balance,
            spam: starting_balance,
            deep: starting_balance,
            usdt: starting_balance,
        };

        // variables to input into order
        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let price = 2 * constants::float_scaling();
        let quantity = 3 * constants::float_scaling();
        let is_bid = true;
        let pay_with_deep = true;
        let expire_timestamp = constants::max_u64();

        // Since both price points are available, SPAM (quote) conversion should be used
        // Alice and Bob execute cross trading in pool 1
        // Alice should have 12 less SPAM, Bob should have 12 more SPAM
        // Alice should have 6 more SUI, Bob should have 6 less SUI
        // DEEP fees will be calculated using the SPAM conversion of 95
        execute_cross_trading<SUI, SPAM>(
            pool1_id,
            alice_balance_manager_id,
            bob_balance_manager_id,
            client_order_id,
            order_type,
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test
        );
        alice_balance.spam = alice_balance.spam - 2 * math::mul(quantity, price);
        alice_balance.sui = alice_balance.sui + 2 * quantity;
        bob_balance.spam = bob_balance.spam + 2 * math::mul(quantity, price);
        bob_balance.sui = bob_balance.sui - 2 * quantity;

        let taker_quantity = quantity;
        let maker_quantity = quantity;
        let maker_fee = math::mul(
            math::mul(constants::maker_fee(), math::mul(price, maker_quantity)),
            95 * constants::float_scaling()
        );
        let taker_fee = math::mul(
            math::mul(constants::taker_fee(), math::mul(price, taker_quantity)),
            95 * constants::float_scaling()
        );

        alice_balance.deep = alice_balance.deep - maker_fee - taker_fee;
        bob_balance.deep = bob_balance.deep - maker_fee - taker_fee;
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );
        check_balance(
            bob_balance_manager_id,
            &bob_balance,
            &mut test
        );

        end(test);
    }

    fun test_master_update_treasury_address(){
        let mut test = begin(OWNER);

        // Treasury address is by default OWNER
        let registry_id = pool_tests::setup_test(OWNER, &mut test);

        let (_, fee_id) = pool_tests::setup_pool_with_default_fees_return_fee<SPAM, SUI>(OWNER, registry_id, false, &mut test);
        check_fee(OWNER, fee_id, &mut test);

        // Set the treasury address to ALICE
        set_treasury_address(
            OWNER,
            registry_id,
            ALICE,
            &mut test
        );

        // First pool creation fee is sent to ALICE
        let (_, fee_id) = pool_tests::setup_pool_with_default_fees_return_fee<SUI, USDC>(OWNER, registry_id, false, &mut test);
        check_fee(ALICE, fee_id, &mut test);

        // Set the treasury address to BOB
        set_treasury_address(
            OWNER,
            registry_id,
            BOB,
            &mut test
        );

        // Second pool creation fee is sent to BOB
        let (_, fee_id) = pool_tests::setup_pool_with_default_fees_return_fee<SPAM, USDC>(OWNER, registry_id, false, &mut test);
        check_fee(BOB, fee_id, &mut test);

        end(test);
    }

    fun test_trader_permission_and_modify_returned(
        error_code: u64,
    ) {
        let mut test = begin(OWNER);
        let registry_id = pool_tests::setup_test(OWNER, &mut test);
        pool_tests::set_time(0, &mut test);
        let starting_balance = 10000 * constants::float_scaling();

        let owner_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            OWNER,
            starting_balance,
            &mut test
        );

        let alice_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            ALICE,
            starting_balance,
            &mut test
        );

        // Create pool and reference pool
        let pool1_id = pool_tests::setup_pool_with_default_fees<SUI, USDC>(OWNER, registry_id, false, &mut test);
        let pool1_reference_id = pool_tests::setup_reference_pool<SUI, DEEP>(OWNER, registry_id, owner_balance_manager_id, 100 * constants::float_scaling(), &mut test);

        // Default price point of 100 deep per base will be added
        pool_tests::add_deep_price_point<SUI, USDC, SUI, DEEP>(
            OWNER,
            pool1_id,
            pool1_reference_id,
            &mut test,
        );

        // Bob tries to authorize himself on Alice's balance manager, will error
        if (error_code == EInvalidOwner) {
            authorize_trader(BOB, alice_balance_manager_id, BOB, &mut test);
        };

        // Alice gives Bob permission to trade on her balance manager
        let bob_trade_cap_id = authorize_trader(ALICE, alice_balance_manager_id, BOB, &mut test);

        // variables to input into order
        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let price = 2 * constants::float_scaling();
        let quantity = 10 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let is_bid = true;
        let pay_with_deep = true;
        let maker_fee = constants::maker_fee();
        let mut alice_balance = ExpectedBalances{
            sui: starting_balance,
            usdc: starting_balance,
            spam: starting_balance,
            deep: starting_balance,
            usdt: starting_balance,
        };

        // Bob places an order with quantity 10 in SUI/USDC pool at a price of 2 using Alice's balance manager
        let order_info = pool_tests::place_limit_order<SUI, USDC>(
            BOB,
            pool1_id,
            alice_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );
        alice_balance.usdc = alice_balance.usdc - math::mul(price, quantity);
        alice_balance.deep = alice_balance.deep - math::mul(
            math::mul(maker_fee, constants::deep_multiplier()),
            quantity
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        let quantity = 5 * constants::float_scaling();

        // Owner places an ask order at the same price matches with 5 of Alice's order
        pool_tests::place_limit_order<SUI, USDC>(
            OWNER,
            pool1_id,
            owner_balance_manager_id,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );
        alice_balance.sui = alice_balance.sui + quantity;

        let new_quantity = 8 * constants::float_scaling();
        let cancelled_quantity = 2 * constants::float_scaling();
        let remaining_quantity = 3 * constants::float_scaling();

        // Bob modifies the order from original quantity of 10 to 8
        // Since quantity of 5 was filled, the effective quantity is 3
        pool_tests::modify_order<SUI, USDC>(
            BOB,
            pool1_id,
            alice_balance_manager_id,
            order_info.order_id(),
            new_quantity,
            &mut test
        );
        alice_balance.usdc = alice_balance.usdc + math::mul(price, cancelled_quantity);
        alice_balance.deep = alice_balance.deep + math::mul(
            math::mul(maker_fee, constants::deep_multiplier()),
            cancelled_quantity
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Alice cancels the order herself, should get correct refund of remaining quantity
        pool_tests::cancel_order<SUI, USDC>(
            ALICE,
            pool1_id,
            alice_balance_manager_id,
            order_info.order_id(),
            &mut test
        );
        alice_balance.usdc = alice_balance.usdc + math::mul(price, remaining_quantity);
        alice_balance.deep = alice_balance.deep + math::mul(
            math::mul(maker_fee, constants::deep_multiplier()),
            remaining_quantity
        );
        check_balance(
            alice_balance_manager_id,
            &alice_balance,
            &mut test
        );

        // Alice revokes Bob's trading permission
        remove_trader(ALICE, alice_balance_manager_id, bob_trade_cap_id, &mut test);

        // Alice revokes Bob's trading permission again, removing a trader not in list will error
        if (error_code == ETradeCapNotInList) {
            remove_trader(ALICE, alice_balance_manager_id, bob_trade_cap_id, &mut test);
        };

        // Bob tries to place an order using Alice's balance manager, will error
        if (error_code == EInvalidTrader) {
            pool_tests::place_limit_order<SUI, USDC>(
                BOB,
                pool1_id,
                alice_balance_manager_id,
                client_order_id,
                order_type,
                constants::self_matching_allowed(),
                price,
                quantity,
                is_bid,
                pay_with_deep,
                expire_timestamp,
                &mut test,
            );
        };

        end(test);
    }

    fun test_get_level_2_range() {
        // There is a reference pool with SUI as base asset and DEEP as quote asset
        // We call get level 2 range for the reference pool, should return correct vectors
        let mut test = begin(OWNER);
        let registry_id = pool_tests::setup_test(OWNER, &mut test);
        pool_tests::set_time(0, &mut test);

        let starting_balance = 10000 * constants::float_scaling();
        let owner_balance_manager_id = balance_manager_tests::create_acct_and_share_with_funds(
            OWNER,
            starting_balance,
            &mut test
        );
        let pool1_reference_id = pool_tests::setup_reference_pool<SUI, DEEP>(OWNER, registry_id, owner_balance_manager_id, 100 * constants::float_scaling(), &mut test);

        // Currently there's a bid order at price 20 with quantity 1
        // OWNER places another bid order in the reference pool at price 20 and quantity 2
        let price = 20 * constants::float_scaling();
        let quantity = 2 * constants::float_scaling();
        let is_bid = true;
        pool_tests::place_limit_order<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            owner_balance_manager_id,
            1,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            false,
            constants::max_u64(),
            &mut test,
        );

        // OWNER places another order in the reference pool at price 30 and quantity 5
        let price = 30 * constants::float_scaling();
        let quantity = 5 * constants::float_scaling();
        let is_bid = true;
        pool_tests::place_limit_order<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            owner_balance_manager_id,
            2,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            false,
            constants::max_u64(),
            &mut test,
        );

        // Get level 2 range for the reference pool, should return correct vectors
        let is_bid = true;
        let (prices, quantities) = get_level2_range<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            0,
            30 * constants::float_scaling(),
            is_bid,
            &mut test
        );
        assert!(prices[0] == 30 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(prices[1] == 20 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(quantities[0] == 5 * constants::float_scaling(), EIncorrectLevel2Quantity);
        assert!(quantities[1] == 3 * constants::float_scaling(), EIncorrectLevel2Quantity);

        // Include price 20 but exclude price 30
        let (prices, quantities) = get_level2_range<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            0,
            20 * constants::float_scaling(),
            is_bid,
            &mut test
        );
        assert!(prices[0] == 20 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(quantities[0] == 3 * constants::float_scaling(), EIncorrectLevel2Quantity);

        // Exclude all prices
        let (prices, quantities) = get_level2_range<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            21 * constants::float_scaling(),
            29 * constants::float_scaling(),
            is_bid,
            &mut test
        );
        assert!(prices.length() == 0, EIncorrectLevel2Price);
        assert!(quantities.length() == 0, EIncorrectLevel2Quantity);

        // Currently there's an ask order at price 180 with quantity 1
        // OWNER places another ask order in the reference pool at price 180 and quantity 2
        let price = 180 * constants::float_scaling();
        let quantity = 2 * constants::float_scaling();
        let is_bid = false;
        pool_tests::place_limit_order<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            owner_balance_manager_id,
            3,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            false,
            constants::max_u64(),
            &mut test,
        );

        // OWNER places another ask order at price 170 and quantity 5
        let price = 170 * constants::float_scaling();
        let quantity = 5 * constants::float_scaling();
        let is_bid = false;
        pool_tests::place_limit_order<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            owner_balance_manager_id,
            4,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            false,
            constants::max_u64(),
            &mut test,
        );

        // Get level 2 range for the reference pool, should return correct vectors
        let is_bid = false;
        let (prices, quantities) = get_level2_range<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            170 * constants::float_scaling(),
            200 * constants::float_scaling(),
            is_bid,
            &mut test
        );

        assert!(prices[0] == 170 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(prices[1] == 180 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(quantities[0] == 5 * constants::float_scaling(), EIncorrectLevel2Quantity);
        assert!(quantities[1] == 3 * constants::float_scaling(), EIncorrectLevel2Quantity);

        // Include price 180 but exclude price 170
        let (prices, quantities) = get_level2_range<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            180 * constants::float_scaling(),
            200 * constants::float_scaling(),
            is_bid,
            &mut test
        );
        assert!(prices[0] == 180 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(quantities[0] == 3 * constants::float_scaling(), EIncorrectLevel2Quantity);

        // Include price 170 but exclude 180
        let (prices, quantities) = get_level2_range<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            170 * constants::float_scaling(),
            179 * constants::float_scaling(),
            is_bid,
            &mut test
        );
        assert!(prices[0] == 170 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(quantities[0] == 5 * constants::float_scaling(), EIncorrectLevel2Quantity);

        // Only the best bid of 30 and best ask of 170 should be returned
        let (bid_prices, bid_quantities, ask_prices, ask_quantities) = get_level2_ticks_from_mid<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            1,
            &mut test
        );
        assert!(bid_prices[0] == 30 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(bid_quantities[0] == 5 * constants::float_scaling(), EIncorrectLevel2Quantity);
        assert!(ask_prices[0] == 170 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(ask_quantities[0] == 5 * constants::float_scaling(), EIncorrectLevel2Quantity);

        // Both bids and asks (2 each) should be returned
        let (bid_prices, bid_quantities, ask_prices, ask_quantities) = get_level2_ticks_from_mid<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            2,
            &mut test
        );
        assert!(bid_prices[0] == 30 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(bid_quantities[0] == 5 * constants::float_scaling(), EIncorrectLevel2Quantity);
        assert!(bid_prices[1] == 20 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(bid_quantities[1] == 3 * constants::float_scaling(), EIncorrectLevel2Quantity);
        assert!(ask_prices[0] == 170 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(ask_quantities[0] == 5 * constants::float_scaling(), EIncorrectLevel2Quantity);
        assert!(ask_prices[1] == 180 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(ask_quantities[1] == 3 * constants::float_scaling(), EIncorrectLevel2Quantity);

        // Should only return 2 bids and 2 asks even though tick is higher
        let (bid_prices, bid_quantities, ask_prices, ask_quantities) = get_level2_ticks_from_mid<SUI, DEEP>(
            OWNER,
            pool1_reference_id,
            3,
            &mut test
        );
        assert!(bid_prices[0] == 30 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(bid_quantities[0] == 5 * constants::float_scaling(), EIncorrectLevel2Quantity);
        assert!(bid_prices[1] == 20 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(bid_quantities[1] == 3 * constants::float_scaling(), EIncorrectLevel2Quantity);
        assert!(ask_prices[0] == 170 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(ask_quantities[0] == 5 * constants::float_scaling(), EIncorrectLevel2Quantity);
        assert!(ask_prices[1] == 180 * constants::float_scaling(), EIncorrectLevel2Price);
        assert!(ask_quantities[1] == 3 * constants::float_scaling(), EIncorrectLevel2Quantity);

        end(test);
    }

    // === Private Helper Functions ===
    fun authorize_trader(
        sender: address,
        balance_manager_id: ID,
        trader: address,
        test: &mut Scenario,
    ): ID {
        test.next_tx(sender);
        {
            let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_cap = balance_manager.mint_trade_cap(test.ctx());
            let trade_cap_id = object::id(&trade_cap);
            transfer::public_transfer(trade_cap, trader);
            return_shared(balance_manager);

            trade_cap_id
        }
    }

    fun remove_trader(
        sender: address,
        balance_manager_id: ID,
        trade_cap_id: ID,
        test: &mut Scenario,
    ) {
        test.next_tx(sender);
        {
            let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            balance_manager.revoke_trade_cap(&trade_cap_id, test.ctx());
            return_shared(balance_manager);
        }
    }

    fun withdraw_and_burn<T>(
        sender: address,
        balance_manager_id: ID,
        withdraw_amount: u64,
        test: &mut Scenario,
    ) {
        test.next_tx(sender);
        {
            let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let coin = balance_manager.withdraw<T>(withdraw_amount, test.ctx());

            coin.burn_for_testing();
            return_shared(balance_manager);
        }
    }

    fun check_fee(
        sender: address,
        fee_id: ID,
        test: &mut Scenario,
    ){
        test.next_tx(sender);
        let fee = test.take_from_sender_by_id<Coin<DEEP>>(fee_id);
        assert!(fee.value() == constants::pool_creation_fee(), 0);
        fee.burn_for_testing();
    }

    fun set_treasury_address(
        sender: address,
        registry_id: ID,
        treasury_address: address,
        test: &mut Scenario,
    ){
        test.next_tx(sender);
        {
            let admin_cap = registry::get_admin_cap_for_testing(test.ctx());
            let mut registry = test.take_shared_by_id<Registry>(registry_id);

            registry::set_treasury_address(
                &mut registry,
                treasury_address,
                &admin_cap,
            );
            test_utils::destroy(admin_cap);
            return_shared(registry);
        }
    }

    fun check_mid_price<BaseAsset, QuoteAsset>(
        pool_id: ID,
        expected_mid_price: u64,
        test: &mut Scenario,
    ){
        test.next_tx(OWNER);
        {
            let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();
            let mid_price = pool::mid_price(&pool, &clock);
            assert!(mid_price == expected_mid_price, 0);
            return_shared(pool);
            return_shared(clock);
        }
    }

    fun burn_deep<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        expected_amount_burned: u64,
        test: &mut Scenario,
    ) {
        test.next_tx(sender);
        {
            deep::share_treasury_for_testing(test.ctx());
        };
        test.next_tx(sender);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let mut treasury = test.take_shared<ProtectedTreasury>();
            let amount_burned = pool::burn_deep<BaseAsset, QuoteAsset>(
                &mut pool,
                &mut treasury,
                test.ctx()
            );
            assert!(amount_burned == expected_amount_burned, 0);
            return_shared(pool);
            return_shared(treasury);
        }
    }

    fun execute_cross_trading<BaseAsset, QuoteAsset>(
        pool_id: ID,
        balance_manager_id_1: ID,
        balance_manager_id_2: ID,
        client_order_id: u64,
        order_type: u8,
        price: u64,
        quantity: u64,
        is_bid: bool,
        pay_with_deep: bool,
        expire_timestamp: u64,
        test: &mut Scenario,
    ) {
        pool_tests::place_limit_order<BaseAsset, QuoteAsset>(
            ALICE,
            pool_id,
            balance_manager_id_1,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            test,
        );
        pool_tests::place_limit_order<BaseAsset, QuoteAsset>(
            BOB,
            pool_id,
            balance_manager_id_2,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            2 * quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            test,
        );
        pool_tests::place_limit_order<BaseAsset, QuoteAsset>(
            ALICE,
            pool_id,
            balance_manager_id_1,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            test,
        );
        withdraw_settled_amounts<BaseAsset, QuoteAsset>(
            BOB,
            pool_id,
            balance_manager_id_2,
            test
        );
    }

    fun check_vault_balances<BaseAsset, QuoteAsset>(
        pool_id: ID,
        expected_balances: &Balances,
        test: &mut Scenario,
    ) {
        test.next_tx(OWNER);
        {
            let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let (vault_base, vault_quote, vault_deep) = pool::vault_balances<BaseAsset, QuoteAsset>(&pool);
            assert!(vault_base == expected_balances.base(), 0);
            assert!(vault_quote == expected_balances.quote(), 0);
            assert!(vault_deep == expected_balances.deep(), 0);

            return_shared(pool);
        }
    }

    fun claim_rebates<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        balance_manager_id: ID,
        test: &mut Scenario,
    ){
        test.next_tx(sender);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let mut my_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_cap = test.take_from_sender<TradeCap>();
            let trade_proof = my_manager.generate_proof_as_trader(&trade_cap, test.ctx());
            pool::claim_rebates<BaseAsset, QuoteAsset>(
                &mut pool,
                &mut my_manager,
                &trade_proof,
                test.ctx()
            );
            test.return_to_sender(trade_cap);
            return_shared(pool);
            return_shared(my_manager);
        }
    }

    fun unstake<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        balance_manager_id: ID,
        test: &mut Scenario,
    ){
        test.next_tx(sender);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let mut my_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_proof = my_manager.generate_proof_as_owner(test.ctx());

            pool::unstake<BaseAsset, QuoteAsset>(
                &mut pool,
                &mut my_manager,
                &trade_proof,
                test.ctx()
            );
            return_shared(pool);
            return_shared(my_manager);
        }
    }

    fun withdraw_settled_amounts<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        balance_manager_id: ID,
        test: &mut Scenario,
    ) {
        test.next_tx(sender);
        {
            let mut my_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let trade_proof = my_manager.generate_proof_as_owner(test.ctx());
            pool::withdraw_settled_amounts<BaseAsset, QuoteAsset>(
                &mut pool,
                &mut my_manager,
                &trade_proof,
            );
            return_shared(my_manager);
            return_shared(pool);
        }
    }

    fun check_balance(
        balance_manager_id: ID,
        expected_balances: &ExpectedBalances,
        test: &mut Scenario,
    ) {
        test.next_tx(OWNER);
        {
            let my_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let sui = balance_manager::balance<SUI>(&my_manager);
            let usdc = balance_manager::balance<USDC>(&my_manager);
            let spam = balance_manager::balance<SPAM>(&my_manager);
            let deep = balance_manager::balance<DEEP>(&my_manager);
            let usdt = balance_manager::balance<USDT>(&my_manager);
            assert!(sui == expected_balances.sui, 0);
            assert!(usdc == expected_balances.usdc, 0);
            assert!(spam == expected_balances.spam, 0);
            assert!(deep == expected_balances.deep, 0);
            assert!(usdt == expected_balances.usdt, 0);

            return_shared(my_manager);
        }
    }

    fun stake<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        balance_manager_id: ID,
        amount: u64,
        test: &mut Scenario,
    ){
        test.next_tx(sender);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let mut my_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_proof = my_manager.generate_proof_as_owner(test.ctx());

            pool::stake<BaseAsset, QuoteAsset>(
                &mut pool,
                &mut my_manager,
                &trade_proof,
                amount,
                test.ctx()
            );
            return_shared(pool);
            return_shared(my_manager);
        }
    }

    fun submit_proposal<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        balance_manager_id: ID,
        taker_fee: u64,
        maker_fee: u64,
        stake_required: u64,
        test: &mut Scenario,
    ){
        test.next_tx(sender);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let mut my_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_proof = my_manager.generate_proof_as_owner(test.ctx());

            pool::submit_proposal<BaseAsset, QuoteAsset>(
                &mut pool,
                &mut my_manager,
                &trade_proof,
                taker_fee,
                maker_fee,
                stake_required,
                test.ctx()
            );
            return_shared(pool);
            return_shared(my_manager);
        }
    }

    fun get_level2_range<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        price_low: u64,
        price_high: u64,
        is_bid: bool,
        test: &mut Scenario,
    ): (vector<u64>, vector<u64>){
        test.next_tx(sender);
        {
            let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let (prices, quantities) = pool.get_level2_range<BaseAsset, QuoteAsset>(
                price_low,
                price_high,
                is_bid,
            );
            return_shared(pool);

            (prices, quantities)
        }
    }

    fun get_level2_ticks_from_mid<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        ticks: u64,
        test: &mut Scenario,
    ): (vector<u64>, vector<u64>, vector<u64>, vector<u64>){
        test.next_tx(sender);
        {
            let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let (bid_prices, bid_quantities, ask_prices, ask_quantities) = pool.get_level2_ticks_from_mid<BaseAsset, QuoteAsset>(
                ticks,
            );
            return_shared(pool);

            (bid_prices, bid_quantities, ask_prices, ask_quantities)
        }
    }
}
