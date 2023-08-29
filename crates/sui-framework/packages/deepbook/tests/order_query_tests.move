// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module deepbook::order_query_tests {

    use std::option::{none, some};
    use std::vector;
    use deepbook::order_query;
    use deepbook::order_query::iter_bids;
    use deepbook::custodian_v2;
    use deepbook::custodian_v2::{AccountCap, account_owner};
    use sui::clock::Clock;
    use sui::coin::mint_for_testing;
    use deepbook::clob_v2;
    use sui::sui::SUI;
    use deepbook::clob_v2::{setup_test, USD, mint_account_cap_transfer, Pool};
    use sui::test_scenario;
    use sui::test_scenario::{next_tx, end, ctx, Scenario};

    const CLIENT_ID_ALICE: u64 = 0;
    const FLOAT_SCALING: u64 = 1000000000;
    const CANCEL_OLDEST: u8 = 0;
    const TIMESTAMP_INF: u64 = ((1u128 << 64 - 1) as u64);

    const OWNER: address = @0xf;
    const ALICE: address = @0xBEEF;
    const BOB: address = @0x1337;

    #[test]
    fun test_order_query_pagination() {
        let scenario = prepare_scenario();
        let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut scenario);
        let page1 = iter_bids(&pool, none(), none(), none(), none());
        assert!(vector::length(order_query::orders(&page1)) == 100, 0);
        assert!(order_query::has_next_page(&page1), 0);

        let page2 = iter_bids(
            &pool,
            order_query::next_tick_level(&page1),
            order_query::next_order_id(&page1),
            none(),
            none()
        );
        assert!(vector::length(order_query::orders(&page2)) == 100, 0);
        assert!(!order_query::has_next_page(&page2), 0);

        test_scenario::return_shared(pool);
        end(scenario);
    }

    #[test]
    fun test_order_query_start_order_id() {
        let scenario = prepare_scenario();
        let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut scenario);
        // test start order id
        let page = iter_bids(&pool, none(), some(51), none<u64>(), none<u64>());
        assert!(vector::length(order_query::orders(&page)) == 100, 0);
        assert!(order_query::has_next_page(&page), 0);

        let page2 = iter_bids(
            &pool,
            order_query::next_tick_level(&page),
            order_query::next_order_id(&page),
            none(),
            none()
        );
        assert!(vector::length(order_query::orders(&page2)) == 50, 0);
        assert!(!order_query::has_next_page(&page2), 0);

        test_scenario::return_shared(pool);
        end(scenario);
    }

    fun prepare_scenario(): Scenario {
        let scenario = test_scenario::begin(@0x1);
        next_tx(&mut scenario, OWNER);
        setup_test(5000000, 2500000, &mut scenario, OWNER);

        next_tx(&mut scenario, ALICE);
        mint_account_cap_transfer(ALICE, test_scenario::ctx(&mut scenario));
        next_tx(&mut scenario, BOB);
        mint_account_cap_transfer(BOB, test_scenario::ctx(&mut scenario));
        next_tx(&mut scenario, ALICE);

        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut scenario);
            let account_cap = test_scenario::take_from_sender<AccountCap>(&scenario);
            let account_cap_user = account_owner(&account_cap);
            let (base_custodian, quote_custodian) = clob_v2::borrow_mut_custodian(&mut pool);
            custodian_v2::deposit(base_custodian, mint_for_testing<SUI>(1000000, ctx(&mut scenario)), account_cap_user);
            custodian_v2::deposit(
                quote_custodian,
                mint_for_testing<USD>(10000000, ctx(&mut scenario)),
                account_cap_user
            );
            test_scenario::return_shared(pool);
            test_scenario::return_to_sender<AccountCap>(&scenario, account_cap);
        };

        // alice places limit orders
        next_tx(&mut scenario, ALICE);
        {
            let n = 1;
            while (n <= 200) {
                let account_cap = test_scenario::take_from_sender<AccountCap>(&scenario);
                let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut scenario);
                let clock = test_scenario::take_shared<Clock>(&mut scenario);
                clob_v2::place_limit_order<SUI, USD>(
                    &mut pool,
                    CLIENT_ID_ALICE,
                    n * FLOAT_SCALING,
                    200,
                    CANCEL_OLDEST,
                    true,
                    TIMESTAMP_INF,
                    0,
                    &clock,
                    &account_cap,
                    ctx(&mut scenario)
                );
                n = n + 1;
                test_scenario::return_shared(clock);
                test_scenario::return_shared(pool);
                test_scenario::return_to_address<AccountCap>(ALICE, account_cap);
                next_tx(&mut scenario, ALICE);
            };
        };
        scenario
    }
}