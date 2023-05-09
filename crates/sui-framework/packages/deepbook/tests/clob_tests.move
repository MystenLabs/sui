// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Tests for the pool module.
/// They are sequential and based on top of each other.
module deepbook::clob_test {
    use std::vector;

    use sui::clock::Clock;
    use sui::coin::{Self, mint_for_testing, burn_for_testing};
    use sui::object;
    use sui::sui::SUI;
    use sui::test_scenario::{Self as test, Scenario, next_tx, ctx, end, TransactionEffects};

    use deepbook::clob::{Self, Pool, Order, USD, account_balance, get_pool_stat, order_id, list_open_orders, mint_account_cap_transfer};
    use deepbook::custodian::{Self, AccountCap};

    const MIN_PRICE: u64 = 0;
    const MAX_PRICE: u64 = ((1u128 << 64 - 1) as u64);
    const MIN_BID_ORDER_ID: u64 = 0;
    const MIN_ASK_ORDER_ID: u64 = 1 << 63;
    const FLOAT_SCALING: u64 = 1000000000;
    const TIMESTAMP_INF: u64 = ((1u128 << 64 - 1) as u64);
    const IMMEDIATE_OR_CANCEL: u8 = 1;
    const FILL_OR_KILL: u8 = 2;
    const POST_OR_ABORT: u8 = 3;
    const E_ORDER_CANNOT_BE_FULLY_FILLED: u64 = 9;

    #[test] fun test_full_transaction() { let _ = test_full_transaction_(scenario()); }

    #[test] fun test_place_limit_order_fill_or_kill() { let _ = test_place_limit_order_fill_or_kill_(scenario()); }

    #[test] fun test_place_limit_order_post_or_abort() { let _ = test_place_limit_order_post_or_abort_(scenario()); }


    #[test] fun test_absorb_all_liquidity_bid_side_with_customized_tick(
    ) { let _ = test_absorb_all_liquidity_bid_side_with_customized_tick_(scenario()); }

    #[test] fun test_absorb_all_liquidity_ask_side_with_customized_tick(
    ) { let _ = test_absorb_all_liquidity_ask_side_with_customized_tick_(scenario()); }

    #[test] fun test_swap_exact_quote_for_base(
    ) { let _ = test_swap_exact_quote_for_base_(scenario()); }

    #[test] fun test_swap_exact_base_for_quote(
    ) { let _ = test_swap_exact_base_for_quote_(scenario()); }

    #[test] fun test_deposit_withdraw() { let _ = test_deposit_withdraw_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid_with_quote_quantity(
    ) { let _ = test_inject_and_match_taker_bid_with_quote_quantity_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid_with_quote_quantity_zero_lot(
    ) { let _ = test_inject_and_match_taker_bid_with_quote_quantity_zero_lot_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid_with_quote_quantity_partial_lot(
    ) { let _ = test_inject_and_match_taker_bid_with_quote_quantity_partial_lot_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid() { let _ = test_inject_and_match_taker_bid_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid_with_maker_order_not_fully_filled(
    ) { let _ = test_inject_and_match_taker_bid_with_maker_order_not_fully_filled_(scenario()); }

    #[test] fun test_inject_and_match_taker_ask() { let _ = test_inject_and_match_taker_ask_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid_with_expiration(
    ) { let _ = test_inject_and_match_taker_bid_with_expiration_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid_with_quote_quantity_and_expiration(
    ) { let _ = test_inject_and_match_taker_bid_with_quote_quantity_and_expiration_(scenario()); }

    #[test] fun test_inject_and_match_taker_ask_with_expiration(
    ) { let _ = test_inject_and_match_taker_ask_with_expiration_(scenario()); }

    #[test] fun test_inject_and_price_limit_affected_match_taker_bid() {
        let _ = test_inject_and_price_limit_affected_match_taker_bid_(
            scenario()
        );
    }

    #[test] fun test_inject_and_price_limit_affected_match_taker_ask() {
        let _ = test_inject_and_price_limit_affected_match_taker_ask_(
            scenario()
        );
    }

    #[test] fun test_remove_order() { let _ = test_remove_order_(scenario()); }

    #[test] fun test_remove_all_orders() { let _ = test_remove_all_orders_(scenario()); }

    #[test] fun test_cancel_and_remove() { let _ = test_cancel_and_remove_(scenario()); }

    #[test] fun test_batch_cancel() { let _ = test_batch_cancel_(scenario()); }

    #[test] fun test_partial_fill_and_cancel() { let _ = test_partial_fill_and_cancel_(scenario()); }

    #[test] fun test_list_open_orders() {
        let _ = test_list_open_orders_(
            scenario()
        );
    }

    #[test] fun get_level2_book_status_bid_side() {
        let _ = get_level2_book_status_bid_side_(
            scenario()
        );
    }

    #[test] fun get_level2_book_status_ask_side() {
        let _ = get_level2_book_status_ask_side_(
            scenario()
        );
    }

    fun get_level2_book_status_bid_side_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 100000;
            let alice_deposit_USDC: u64 = 100000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 4 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 4 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 3 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order_with_expiration(&mut pool, 2 * FLOAT_SCALING, 1000, true, 0, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test get_level2_book_status_bid_side
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let clock = test::take_shared<Clock>(&mut test);
            let order = clob::get_order_status(&pool, order_id(0, true), &account_cap);
            let order_cmp = clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, true, account_cap_user);
            assert!(order == &order_cmp, 0);
            let (prices, depth) = clob::get_level2_book_status_bid_side(
                &pool,
                1 * FLOAT_SCALING,
                15 * FLOAT_SCALING,
                &clock
            );
            let prices_cmp = vector<u64>[2 * FLOAT_SCALING, 3 * FLOAT_SCALING, 4 * FLOAT_SCALING, 5 * FLOAT_SCALING];
            let depth_cmp = vector<u64>[1000, 1000, 1000, 1000];
            assert!(prices == prices_cmp, 0);
            assert!(depth == depth_cmp, 0);
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun get_level2_book_status_ask_side_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 100000;
            let alice_deposit_USDC: u64 = 100000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 4 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 4 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 3 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order_with_expiration(&mut pool, 2 * FLOAT_SCALING, 1000, false, 0, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test get_level2_book_status_ask_side
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let clock = test::take_shared<Clock>(&mut test);
            let order = clob::get_order_status(&pool, order_id(0, false), &account_cap);
            let order_cmp = clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user);
            assert!(order == &order_cmp, 0);
            let (prices, depth) = clob::get_level2_book_status_ask_side(
                &pool,
                1 * FLOAT_SCALING,
                10 * FLOAT_SCALING,
                &clock
            );
            let prices_cmp = vector<u64>[2 * FLOAT_SCALING, 3 * FLOAT_SCALING, 4 * FLOAT_SCALING, 5 * FLOAT_SCALING];
            let depth_cmp = vector<u64>[1000, 1000, 1000, 1000];
            assert!(prices == prices_cmp, 0);
            assert!(depth == depth_cmp, 0);
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_list_open_orders_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test list_open_orders
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let open_orders = list_open_orders(&pool, &account_cap);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, false, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                1500,
                MAX_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1500, 0);
            assert!(quote_quantity_filled == 4500 + 10 + 13, 0);
            test::return_shared(pool);
        };

        // test list_open_orders after match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let open_orders = list_open_orders(&pool, &account_cap);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 10 * FLOAT_SCALING, 10000, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test list_open_orders before match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let open_orders = list_open_orders(&pool, &account_cap);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, true, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, true, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, true, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (ask side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_ask(
                &mut pool,
                1500,
                MIN_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1500, 0);
            assert!(quote_quantity_filled == 6000 - 13 - 13 - 5, 0);
            test::return_shared(pool);
        };

        // test list_open_orders after match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let open_orders = list_open_orders(&pool, &account_cap);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(2, 2 * FLOAT_SCALING, 500, true, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_deposit_withdraw_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_withdraw_WSUI: u64 = 5000;
            let alice_deposit_USDC: u64 = 10000;
            let alice_withdraw_USDC: u64 = 1000;
            clob::deposit_base(&mut pool, mint_for_testing<SUI>(alice_deposit_WSUI, ctx(&mut test)), &account_cap);
            clob::deposit_quote(&mut pool, mint_for_testing<USD>(alice_deposit_USDC, ctx(&mut test)), &account_cap);
            burn_for_testing(clob::withdraw_base(&mut pool, alice_withdraw_WSUI, &account_cap, ctx(&mut test)));
            burn_for_testing(clob::withdraw_quote(&mut pool, alice_withdraw_USDC, &account_cap, ctx(&mut test)));
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance(
                base_custodian,
                account_cap_user,
                alice_deposit_WSUI - alice_withdraw_WSUI,
                0
            );
            custodian::assert_user_balance(
                quote_custodian,
                account_cap_user,
                alice_deposit_USDC - alice_withdraw_USDC,
                0
            );
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_batch_cancel_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        // setup pool and custodian
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);

            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 10 * FLOAT_SCALING, 10000, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let orders = vector::empty<u64>();
            vector::push_back(&mut orders, 0);
            vector::push_back(&mut orders, 1);
            vector::push_back(&mut orders, MIN_ASK_ORDER_ID);
            clob::batch_cancel_order(&mut pool, orders, &account_cap);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 10 * FLOAT_SCALING);
            };

            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_partial_fill_and_cancel_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner: address = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(
                base_custodian,
                mint_for_testing<SUI>(1000 * 100000000, ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                quote_custodian,
                mint_for_testing<USD>(10000 * 100000000, ctx(&mut test)),
                account_cap_user
            );
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // alice places limit orders
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                10 * FLOAT_SCALING,
                1000 * 100000000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 7400 * 100000000, 2600 * 100000000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 1000 * 100000000);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, bob);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, _) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(
                base_custodian,
                mint_for_testing<SUI>(300 * 100000000, ctx(&mut test)),
                account_cap_user
            );
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 300 * 100000000, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(bob, account_cap);
        };

        // bob places market order
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                300 * 100000000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, bob);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 0);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 1400 * 100000000, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(bob, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 4 * FLOAT_SCALING, 100 * 100000000, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 4 * FLOAT_SCALING, 200 * 100000000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 4 * FLOAT_SCALING, &open_orders);
            };

            clob::cancel_order<SUI, USD>(&mut pool, 1, &account_cap);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 4 * FLOAT_SCALING, 200 * 100000000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 4 * FLOAT_SCALING, &open_orders);
            };

            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        end(test)
    }

    fun test_full_transaction_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner: address = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(base_custodian, mint_for_testing<SUI>(1000, ctx(&mut test)), account_cap_user);
            custodian::deposit(quote_custodian, mint_for_testing<USD>(10000, ctx(&mut test)), account_cap_user);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // alice places limit orders
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                300,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                20 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance(base_custodian, account_cap_user, 0, 1000);
            custodian::assert_user_balance(quote_custodian, account_cap_user, 5500, 4500);
            let (base_avail, base_locked, quote_avail, quote_locked) = account_balance(&pool, &account_cap);
            assert!(base_avail == 0, 0);
            assert!(base_locked == 1000, 0);
            assert!(quote_avail == 5500, 0);
            assert!(quote_locked == 4500, 0);
            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // bob places market order
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test::take_shared<Clock>(&mut test);
            let (coin1, coin2) = clob::place_market_order<SUI, USD>(&mut pool, 600, false,
                mint_for_testing<SUI>(600, ctx(&mut test)),
                mint_for_testing<USD>(0, ctx(&mut test)),
                &clock,
                ctx(&mut test));
            assert!(coin::value<SUI>(&coin1) == 0, 0);
            assert!(coin::value<USD>(&coin2) == 2700 - 14, 0);
            burn_for_testing(coin1);
            burn_for_testing(coin2);
            test::return_shared(pool);
            test::return_shared(clock);
        };
        end(test)
    }

    fun test_place_limit_order_fill_or_kill_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner: address = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(base_custodian, mint_for_testing<SUI>(1000, ctx(&mut test)), account_cap_user);
            custodian::deposit(quote_custodian, mint_for_testing<USD>(10000, ctx(&mut test)), account_cap_user);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // alice places limit orders
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                300,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                20 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance(base_custodian, account_cap_user, 0, 1000);
            custodian::assert_user_balance(quote_custodian, account_cap_user, 5500, 4500);
            let (base_avail, base_locked, quote_avail, quote_locked) = account_balance(&pool, &account_cap);
            assert!(base_avail == 0, 0);
            assert!(base_locked == 1000, 0);
            assert!(quote_avail == 5500, 0);
            assert!(quote_locked == 4500, 0);
            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(base_custodian, mint_for_testing<SUI>(1000, ctx(&mut test)), account_cap_user);
            custodian::deposit(quote_custodian, mint_for_testing<USD>(10000, ctx(&mut test)), account_cap_user);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // bob places limit order of FILL_OR_KILL
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let clock = test::take_shared<Clock>(&mut test);
            let (base_quantity_filled, quote_quantity_filled, is_placed, order_id) = clob::place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                400,
                false,
                TIMESTAMP_INF,
                FILL_OR_KILL,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            assert!(base_quantity_filled == 400, 0);
            assert!(quote_quantity_filled == 1990, 0);
            assert!(is_placed == false, 0);
            assert!(order_id == 0, 0);

            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(bob, account_cap);
        };

        // check bob's balance after the limit order is matched
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let clock = test::take_shared<Clock>(&mut test);

            let  (base_avail, base_locked, quote_avail, quote_locked) = account_balance<SUI, USD>(&pool, &account_cap);
            assert!(base_avail == 600, 0);
            assert!(base_locked == 0, 0);
            assert!(quote_avail == 11990, 0);
            assert!(quote_locked == 0, 0);

            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(bob, account_cap);
        };
        end(test)
    }

    fun test_place_limit_order_post_or_abort_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner: address = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(base_custodian, mint_for_testing<SUI>(1000, ctx(&mut test)), account_cap_user);
            custodian::deposit(quote_custodian, mint_for_testing<USD>(10000, ctx(&mut test)), account_cap_user);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // alice places limit orders
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                300,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                20 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance(base_custodian, account_cap_user, 0, 1000);
            custodian::assert_user_balance(quote_custodian, account_cap_user, 5500, 4500);
            let (base_avail, base_locked, quote_avail, quote_locked) = account_balance(&pool, &account_cap);
            assert!(base_avail == 0, 0);
            assert!(base_locked == 1000, 0);
            assert!(quote_avail == 5500, 0);
            assert!(quote_locked == 4500, 0);
            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(base_custodian, mint_for_testing<SUI>(1000, ctx(&mut test)), account_cap_user);
            custodian::deposit(quote_custodian, mint_for_testing<USD>(10000, ctx(&mut test)), account_cap_user);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // bob places limit order of POST OR ABORT
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let clock = test::take_shared<Clock>(&mut test);
            let (base_quantity_filled, quote_quantity_filled, is_placed, order_id) = clob::place_limit_order<SUI, USD>(
                &mut pool,
                6 * FLOAT_SCALING,
                400,
                false,
                TIMESTAMP_INF,
                POST_OR_ABORT,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            assert!(base_quantity_filled == 0, 0);
            assert!(quote_quantity_filled == 0, 0);
            assert!(is_placed == true, 0);
            assert!(order_id == MIN_ASK_ORDER_ID + 1, 0);

            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(bob, account_cap);
        };

        // check bob's balance
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let clock = test::take_shared<Clock>(&mut test);

            let  (base_avail, base_locked, quote_avail, quote_locked) = account_balance<SUI, USD>(&pool, &account_cap);
            assert!(base_avail == 600, 0);
            assert!(base_locked == 400, 0);
            assert!(quote_avail == 10000, 0);
            assert!(quote_locked == 0, 0);

            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(bob, account_cap);
        };
        end(test)
    }

    fun test_absorb_all_liquidity_bid_side_with_customized_tick_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test_with_tick_lot(5000000, 2500000, 1_00_000_000, 10, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(base_custodian, mint_for_testing<SUI>(1000, ctx(&mut test)), account_cap_user);
            custodian::deposit(quote_custodian, mint_for_testing<USD>(10000, ctx(&mut test)), account_cap_user);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // alice places limit orders
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                300,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                20 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance(base_custodian, account_cap_user, 0, 1000);
            custodian::assert_user_balance(quote_custodian, account_cap_user, 5500, 4500);
            let (base_avail, base_locked, quote_avail, quote_locked) = account_balance(&pool, &account_cap);
            assert!(base_avail == 0, 0);
            assert!(base_locked == 1000, 0);
            assert!(quote_avail == 5500, 0);
            assert!(quote_locked == 4500, 0);
            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // bob places market order
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test::take_shared<Clock>(&mut test);
            let (coin1, coin2) = clob::place_market_order<SUI, USD>(&mut pool, 2000, false,
                mint_for_testing<SUI>(2000, ctx(&mut test)),
                mint_for_testing<USD>(0, ctx(&mut test)),
                &clock,
                ctx(&mut test));
            assert!(coin::value<SUI>(&coin1) == 500, 0);
            assert!(coin::value<USD>(&coin2) == 4477, 0);
            burn_for_testing(coin1);
            burn_for_testing(coin2);
            test::return_shared(pool);
            test::return_shared(clock);
        };
        end(test)
    }

    fun test_absorb_all_liquidity_ask_side_with_customized_tick_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner: address = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test_with_tick_lot(5000000, 2500000, 1_00_000_000, 10, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(base_custodian, mint_for_testing<SUI>(10000, ctx(&mut test)), account_cap_user);
            custodian::deposit(quote_custodian, mint_for_testing<USD>(10000, ctx(&mut test)), account_cap_user);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // alice places limit orders
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                500,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                500,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                1 * FLOAT_SCALING,
                1000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance(base_custodian, account_cap_user, 8000, 2000);
            custodian::assert_user_balance(quote_custodian, account_cap_user, 9000, 1000);
            let (base_avail, base_locked, quote_avail, quote_locked) = account_balance(&pool, &account_cap);
            assert!(base_avail == 8000, 0);
            assert!(base_locked == 2000, 0);
            assert!(quote_avail == 9000, 0);
            assert!(quote_locked == 1000, 0);
            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // bob places market order
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test::take_shared<Clock>(&mut test);
            let (coin1, coin2) = clob::place_market_order<SUI, USD>(&mut pool, 5000, true,
                mint_for_testing<SUI>(10000, ctx(&mut test)),
                mint_for_testing<USD>(10000, ctx(&mut test)),
                &clock,
                ctx(&mut test));
            assert!(coin::value<SUI>(&coin1) == 12000, 0);
            assert!(coin::value<USD>(&coin2) == 10000 - (7000 + 36), 0);
            burn_for_testing(coin1);
            burn_for_testing(coin2);
            test::return_shared(pool);
            test::return_shared(clock);
        };
        end(test)
    }

    fun test_swap_exact_quote_for_base_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test::take_shared<Clock>(&mut test);
            let (base_coin, quote_coin, _) = clob::swap_exact_quote_for_base(
                &mut pool,
                4500,
                &clock,
                mint_for_testing<USD>(4500, ctx(&mut test)),
                ctx(&mut test)
            );
            assert!(coin::value(&base_coin) == 1000 + 495, 0);
            assert!(coin::value(&quote_coin) == 2, 0);
            burn_for_testing(base_coin);
            burn_for_testing(quote_coin);
            test::return_shared(clock);
            test::return_shared(pool);
        };
        end(test)
    }

    fun test_swap_exact_base_for_quote_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 10 * FLOAT_SCALING, 10000, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test::take_shared<Clock>(&mut test);
            let (base_coin, quote_coin, _) = clob::swap_exact_base_for_quote(
                &mut pool,
                1500,
                mint_for_testing<SUI>(1500, ctx(&mut test)),
                mint_for_testing<USD>(0,  ctx(&mut test)),
                &clock,
                ctx(&mut test)
            );
            assert!(coin::value(&base_coin) == 0, 0);
            assert!(coin::value(&quote_coin) == 5969, 0);
            burn_for_testing(base_coin);
            burn_for_testing(quote_coin);
            test::return_shared(clock);
            test::return_shared(pool);
        };
        end(test)
    }

    fun test_cancel_and_remove_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10;
            let alice_deposit_USDC: u64 = 100;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);

            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 20 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(3, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 35, 65);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10);

            // check usr open orders before cancel
            {
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            clob::cancel_order(&mut pool, 0, &account_cap);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                // check tick level from pool after remove order
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
                // check usr open orders after remove order bid order of sequence_id 0
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 35 + 10, 65 - 10);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            clob::cancel_order(&mut pool, 1, &account_cap);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 35 + 10 + 15, 65 - 10 - 15);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            clob::cancel_order(&mut pool, MIN_ASK_ORDER_ID, &account_cap);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 20 * FLOAT_SCALING);
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 35 + 10 + 15, 65 - 10 - 15);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 10, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_with_quote_quantity_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test inject limit order and match (bid side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8000, 2000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid_with_quote_quantity(
                &mut pool,
                4500,
                MAX_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1000 + 495, 0);
            assert!(quote_quantity_filled == 4498, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap_alice = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user_alice = object::id(&account_cap_alice);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user_alice, 4498 - 10 - 13 + 5 + 6, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user_alice, 8000, 500 + 5);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 5, false, account_cap_user_alice)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };

            let open_orders = list_open_orders(&pool, &account_cap_alice);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 5 * FLOAT_SCALING, 5, false, account_cap_user_alice)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap_alice);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_with_quote_quantity_zero_lot_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test_with_tick_lot(5000000, 2500000, 1 * FLOAT_SCALING, 100, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid_with_quote_quantity(
                &mut pool,
                // needs 201 to fill 1 lot
                200,
                MAX_PRICE,
                0,
            );
            assert!(base_quantity_filled == 0, 0);
            assert!(quote_quantity_filled == 0, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap_alice = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user_alice = object::id(&account_cap_alice);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user_alice, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user_alice, 8000, 2000);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, false, account_cap_user_alice)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };

            let open_orders = list_open_orders(&pool, &account_cap_alice);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, false, account_cap_user_alice)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap_alice);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_with_quote_quantity_partial_lot_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test_with_tick_lot(5000000, 2500000, 1 * FLOAT_SCALING, 10, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid_with_quote_quantity(
                &mut pool,
                4500,
                MAX_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1000 + 490, 0);
            assert!(quote_quantity_filled == 4473, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap_alice = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user_alice = object::id(&account_cap_alice);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user_alice, 4473 - 10 - 13 + 5 + 6, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user_alice, 8000, 500 + 10);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 10, false, account_cap_user_alice)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };

            let open_orders = list_open_orders(&pool, &account_cap_alice);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 5 * FLOAT_SCALING, 10, false, account_cap_user_alice)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap_alice);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8000, 2000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                1500,
                MAX_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1500, 0);
            assert!(quote_quantity_filled == 4500 + 10 + 13, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap_alice = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user_alice = object::id(&account_cap_alice);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user_alice, 4500 + 5 + 6, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user_alice, 8000, 500);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap_alice);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_with_maker_order_not_fully_filled_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8000, 2000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                1250,
                MAX_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1250, 0);
            assert!(quote_quantity_filled == 3267, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap_alice = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user_alice = object::id(&account_cap_alice);

            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user_alice, 8000, 750);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user_alice, 3258, 10000);

            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 250, false, account_cap_user_alice)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap_alice);
        };
        end(test)
    }

    fun test_inject_and_match_taker_ask_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test inject limit order and match (ask side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 10 * FLOAT_SCALING, 10000, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 3000, 7000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test match (ask side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_ask(
                &mut pool,
                1500,
                MIN_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1500, 0);
            assert!(quote_quantity_filled == 6000 - 13 - 13 - 5, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(
                quote_custodian,
                account_cap_user,
                3000 + 6 + 6 + 2,
                7000 - 2500 - 2500 - 1000
            );
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 1500, 10000);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_with_quote_quantity_and_expiration_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        // test inject limit order and match (bid side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                5 * FLOAT_SCALING,
                500,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                500,
                false,
                0,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                1 * FLOAT_SCALING,
                10000,
                true,
                0,
                &account_cap,
                ctx(&mut test)
            );
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8000, 2000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        0,
                        5 * FLOAT_SCALING,
                        500,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(1, 2 * FLOAT_SCALING, 500, false, account_cap_user, 0)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        2,
                        2 * FLOAT_SCALING,
                        1000,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid_with_quote_quantity(
                &mut pool,
                4500,
                MAX_PRICE,
                1,
            );
            assert!(base_quantity_filled == 1000 + 495, 0);
            assert!(quote_quantity_filled == 4498, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            // rebate fee in base asset 3
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 4498 - 10 - 13 + 5 + 6, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8500, 5);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 5, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_with_expiration_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        // test inject limit order and match (bid side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                5 * FLOAT_SCALING,
                500,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                500,
                false,
                0,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                1 * FLOAT_SCALING,
                10000,
                true,
                0,
                &account_cap,
                ctx(&mut test)
            );
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test::take_shared<Clock>(&test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8000, 2000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        0,
                        5 * FLOAT_SCALING,
                        500,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(1, 2 * FLOAT_SCALING, 500, false, account_cap_user, 0)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        2,
                        2 * FLOAT_SCALING,
                        1000,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                1500,
                MAX_PRICE,
                1,
            );
            assert!(base_quantity_filled == 1500, 0);
            assert!(quote_quantity_filled == 4500 + 10 + 13, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            // rebate fee in base asset 3
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 4500 + 5 + 6, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8500, 0);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 5 * FLOAT_SCALING);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_inject_and_match_taker_ask_with_expiration_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test inject limit order and match (ask side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                5 * FLOAT_SCALING,
                500,
                true,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                5 * FLOAT_SCALING,
                1000,
                true,
                0,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                true,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                10 * FLOAT_SCALING,
                10000,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 500, 9500);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        0,
                        5 * FLOAT_SCALING,
                        500,
                        true,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        1,
                        5 * FLOAT_SCALING,
                        1000,
                        true,
                        account_cap_user,
                        0,
                    )
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        2,
                        2 * FLOAT_SCALING,
                        1000,
                        true,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        0,
                        10 * FLOAT_SCALING,
                        10000,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (ask side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_ask(
                &mut pool,
                1500,
                MIN_PRICE,
                1,
            );
            assert!(base_quantity_filled == 1500, 0);
            assert!(quote_quantity_filled == 4500 - 13 - 10, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(
                quote_custodian,
                account_cap_user,
                5500 + 6 + 5,
                9500 - 2500 - 5000 - 2000
            );
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 1500, 10000);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 2 * FLOAT_SCALING);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_inject_and_price_limit_affected_match_taker_bid_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xFF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 100;
            let alice_deposit_USDC: u64 = 10;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 85, 15);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, false, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match with price limit (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                20,
                5 * FLOAT_SCALING,
                0
            );
            assert!(base_quantity_filled == 15, 0);
            assert!(quote_quantity_filled == 45, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 45, 10);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 85, 0);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 5 * FLOAT_SCALING);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        end(test)
    }

    fun test_inject_and_price_limit_affected_match_taker_ask_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        let owner = @0xFF;
        // setup pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10;
            let alice_deposit_USDC: u64 = 100;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test inject limit order and match (ask side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            // let account_cap_user = get_account_cap_user(&account_cap);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 20 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&mut pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 55, 45);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match with price limit (ask side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_ask(
                &mut pool,
                10,
                3 * FLOAT_SCALING,
                0,
            );
            assert!(base_quantity_filled == 5, 0);
            assert!(quote_quantity_filled == 25, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 55, 20);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 5, 10);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_remove_order_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10;
            let alice_deposit_USDC: u64 = 100;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);

            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 20 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(3, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            // check usr open orders before cancel
            {
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
                let user_open_orders = clob::get_usr_open_orders(&mut pool, account_cap_user);
                clob::check_usr_open_orders(user_open_orders, &usr_open_orders_cmp);
            };

            clob::test_remove_order(&mut pool, 0, 0, true, account_cap_user);
            {
                // check tick level from pool after remove order
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
                // check usr open orders after remove order
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                let user_open_orders = clob::get_usr_open_orders(&mut pool, account_cap_user);
                clob::check_usr_open_orders(user_open_orders, &usr_open_orders_cmp);
            };

            clob::test_remove_order(&mut pool, 0, 1, true, account_cap_user);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                clob::check_usr_open_orders(
                    clob::get_usr_open_orders(&mut pool, account_cap_user),
                    &usr_open_orders_cmp
                );
            };

            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_remove_all_orders_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10;
            let alice_deposit_USDC: u64 = 100;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);

            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 20 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(3, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            // check usr open orders before cancel
            {
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
                let user_open_orders = clob::get_usr_open_orders(&mut pool, account_cap_user);
                clob::check_usr_open_orders(user_open_orders, &usr_open_orders_cmp);
            };

            clob::cancel_all_orders(&mut pool, &account_cap);
            {
                let usr_open_orders_cmp = vector::empty<u64>();
                let user_open_orders = clob::get_usr_open_orders(&mut pool, account_cap_user);
                clob::check_usr_open_orders(user_open_orders, &usr_open_orders_cmp);

                // check tick level after remove
                let (_, _, bids, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
                clob::check_empty_tick_level(bids, 2 * FLOAT_SCALING);
                clob::check_empty_tick_level(asks, 20 * FLOAT_SCALING);
                let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
                custodian::assert_user_balance(base_custodian, account_cap_user, 10, 0);
                custodian::assert_user_balance(quote_custodian, account_cap_user, 100, 0);
                let (base_avail, base_locked, quote_avail, quote_locked) = account_balance(&pool, &account_cap);
                assert!(base_avail == 10, 0);
                assert!(base_locked == 0, 0);
                assert!(quote_avail == 100, 0);
                assert!(quote_locked == 0, 0);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun scenario(): Scenario { test::begin(@0x1) }

    fun people(): (address, address) { (@0xBEEF, @0x1337) }
}
