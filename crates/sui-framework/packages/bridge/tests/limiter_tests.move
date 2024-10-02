// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module bridge::limiter_tests {

    use bridge::{
        chain_ids,
        limiter::{
            check_and_record_sending_transfer, make_transfer_limiter,
            max_transfer_limit, new,
            transfer_limits_mut, total_amount, transfer_records,
            update_route_limit, usd_value_multiplier,
        },
        treasury::{Self, BTC, ETH, USDC, USDT},
    };

    use sui::clock;
    use sui::test_scenario;
    use sui::test_utils::{assert_eq, destroy};

    #[test]
    fun test_24_hours_windows() {
        let mut limiter = make_transfer_limiter();

        let route = chain_ids::get_route(chain_ids::sui_custom(), chain_ids::eth_sepolia());

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut treasury = treasury::mock_for_test(ctx);

        // Global transfer limit is 100M USD
        limiter.transfer_limits_mut().insert(route, 100_000_000 * usd_value_multiplier());
        // Notional price for ETH is 5 USD
        let id = treasury::token_id<ETH>(&treasury);
        treasury.update_asset_notional_price(id, 5 * usd_value_multiplier());

        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(1706288001377);

        // transfer 10000 ETH every hour, the totol should be 10000 * 5
        assert!(
            limiter.check_and_record_sending_transfer<ETH>(
                &treasury,
                &clock,
                route,
                10_000 * treasury.decimal_multiplier<ETH>(),
            ),
            0,
        );

        let record = limiter.transfer_records().get(&route);
        assert!(record.total_amount() == 10000 * 5 * usd_value_multiplier());

        // transfer 1000 ETH every hour for 50 hours, the 24 hours totol should be 24000 * 10
        let mut i = 0;
        while (i < 50) {
            clock.increment_for_testing(60 * 60 * 1000);
            assert!(
                limiter.check_and_record_sending_transfer<ETH>(
                    &treasury,
                    &clock,
                    route,
                    1_000 * treasury.decimal_multiplier<ETH>(),
                ),
                0,
            );
            i = i + 1;
        };
        let record = limiter.transfer_records().get(&route);
        let mut expected_value = 24000 * 5 * usd_value_multiplier();
        assert_eq(record.total_amount(), expected_value);

        // transfer 1000 * i ETH every hour for 24 hours, the 24 hours
        // totol should be 300 * 1000 * 5
        let mut i = 0;
        // At this point, every hour in past 24 hour has value $5000.
        // In each iteration, the old $5000 gets replaced with (i * 5000)
        while (i < 24) {
            clock.increment_for_testing(60 * 60 * 1000);
            assert!(
                limiter.check_and_record_sending_transfer<ETH>(
                    &treasury,
                    &clock,
                    route,
                    1_000 * treasury.decimal_multiplier<ETH>() * (i + 1),
                ),
                0
            );

            let record = limiter.transfer_records().get(&route);

            expected_value = expected_value + 1000 * 5 * i * usd_value_multiplier();
            assert_eq(record.total_amount(), expected_value);
            i = i + 1;
        };

        let record = limiter.transfer_records().get(&route);
        assert_eq(record.total_amount(), 300 * 1000 * 5 * usd_value_multiplier());

        destroy(limiter);
        destroy(treasury);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_24_hours_windows_multiple_route() {
        let mut limiter = make_transfer_limiter();

        let route = chain_ids::get_route(chain_ids::sui_custom(), chain_ids::eth_sepolia());
        let route2 = chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_custom());

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut treasury = treasury::mock_for_test(ctx);

        // Global transfer limit is 1M USD
        limiter.transfer_limits_mut().insert(route, 1_000_000 * usd_value_multiplier());
        limiter.transfer_limits_mut().insert(route2, 500_000 * usd_value_multiplier());
        // Notional price for ETH is 5 USD
        let id = treasury::token_id<ETH>(&treasury);
        treasury.update_asset_notional_price(id, 5 * usd_value_multiplier());

        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(1706288001377);

        // Transfer 10000 ETH on route 1
        assert!(
            limiter.check_and_record_sending_transfer<ETH>(
                &treasury,
                &clock,
                route,
                10_000 * treasury::decimal_multiplier<ETH>(&treasury),
            ),
            0,
        );
        // Transfer 50000 ETH on route 2
        assert!(
            limiter.check_and_record_sending_transfer<ETH>(
                &treasury,
                &clock,
                route2,
                50_000 * treasury::decimal_multiplier<ETH>(&treasury),
            ),
            0,
        );

        let record = limiter.transfer_records().get(&route);
        assert!(record.total_amount() == 10000 * 5 * usd_value_multiplier());

        let record = limiter.transfer_records().get(&route2);
        assert!(record.total_amount() == 50000 * 5 * usd_value_multiplier());

        destroy(limiter);
        destroy(treasury);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_exceed_limit() {
        let mut limiter = make_transfer_limiter();

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut treasury = treasury::mock_for_test(ctx);

        let route = chain_ids::get_route(chain_ids::sui_custom(), chain_ids::eth_sepolia());
        // Global transfer limit is 1M USD
        limiter.transfer_limits_mut().insert(route, 1_000_000 * usd_value_multiplier());
        // Notional price for ETH is 10 USD
        let id = treasury::token_id<ETH>(&treasury);
        treasury.update_asset_notional_price(id, 10 * usd_value_multiplier());

        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(1706288001377);

        assert!(
            limiter.check_and_record_sending_transfer<ETH>(
                &treasury,
                &clock,
                route,
                90_000 * treasury::decimal_multiplier<ETH>(&treasury),
            ),
            0,
        );

        let record = limiter.transfer_records().get(&route);
        assert_eq(record.total_amount(), 90000 * 10 * usd_value_multiplier());

        clock.increment_for_testing(60 * 60 * 1000);
        assert!(
            limiter.check_and_record_sending_transfer<ETH>(
                &treasury,
                &clock,
                route,
                10_000 * treasury::decimal_multiplier<ETH>(&treasury),
            ),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        assert_eq(record.total_amount(), 100000 * 10 * usd_value_multiplier());

        // Tx should fail with a tiny amount because the limit is hit
        assert!(
            !limiter.check_and_record_sending_transfer<ETH>(&treasury, &clock, route, 1),
            0,
        );
        assert!(
            !limiter.check_and_record_sending_transfer<ETH>(
                &treasury,
                &clock,
                route,
                90_000 * treasury::decimal_multiplier<ETH>(&treasury),
            ),
            0,
        );

        // Fast forward 23 hours, now the first 90k should be discarded
        clock.increment_for_testing(60 * 60 * 1000 * 23);
        assert!(
            limiter.check_and_record_sending_transfer<ETH>(
                &treasury,
                &clock,
                route,
                90_000 * treasury::decimal_multiplier<ETH>(&treasury),
            ),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        assert_eq(record.total_amount(), 100000 * 10 * usd_value_multiplier());

        // But now limit is hit again
        assert!(
            !limiter.check_and_record_sending_transfer<ETH>(&treasury, &clock, route, 1),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        assert_eq(record.total_amount(), 100000 * 10 * usd_value_multiplier());

        destroy(limiter);
        destroy(treasury);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = bridge::limiter::ELimitNotFoundForRoute)]
    fun test_limiter_does_not_limit_receiving_transfers() {
        let mut limiter = new();

        let route = chain_ids::get_route(chain_ids::sui_mainnet(), chain_ids::eth_mainnet());
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = scenario.ctx();
        let treasury = treasury::mock_for_test(ctx);
        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(1706288001377);
        // We don't limit sui -> eth transfers. This aborts with `ELimitNotFoundForRoute`
        limiter.check_and_record_sending_transfer<ETH>(
            &treasury,
            &clock,
            route, 1 * treasury::decimal_multiplier<ETH>(&treasury),
        );
        destroy(limiter);
        destroy(treasury);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_limiter_basic_op() {
        // In this test we use very simple number for easier calculation.
        let mut limiter = make_transfer_limiter();

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut treasury = treasury::mock_for_test(ctx);

        let route = chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet());
        // Global transfer limit is 100 USD
        limiter.transfer_limits_mut().insert(route, 100 * usd_value_multiplier());
        // BTC: $10, ETH: $2.5, USDC: $1, USDT: $0.5
        let id = treasury::token_id<BTC>(&treasury);
        treasury.update_asset_notional_price(id, 10 * usd_value_multiplier());
        let id = treasury::token_id<ETH>(&treasury);
        let eth_price = 250000000;
        treasury.update_asset_notional_price(id, eth_price);
        let id = treasury::token_id<USDC>(&treasury);
        treasury.update_asset_notional_price(id, 1 * usd_value_multiplier());
        let id = treasury::token_id<USDT>(&treasury);
        treasury.update_asset_notional_price(id, 50000000);

        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(36082800000); // hour 10023

        // hour 0 (10023): $15 * 2.5 = $37.5
        // 15 eth = $37.5
        assert!(
            limiter.check_and_record_sending_transfer<ETH>(
                &treasury,
                &clock,
                route,
                15 * treasury::decimal_multiplier<ETH>(&treasury),
            ),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        assert_eq(record.hour_head(), 10023);
        assert_eq(record.hour_tail(), 10000);
        assert!(
            record.per_hour_amounts() ==
            &vector[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                15 * eth_price,
            ],
            0,
        );
        assert_eq(record.total_amount(), 15 * eth_price);

        // hour 0 (10023): $37.5 + $10 = $47.5
        // 10 uddc = $10
        assert!(
            limiter.check_and_record_sending_transfer<USDC>(
                &treasury,
                &clock,
                route, 10 * treasury::decimal_multiplier<USDC>(&treasury),
            ),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        assert_eq(record.hour_head(), 10023);
        assert_eq(record.hour_tail(), 10000);
        let expected_notion_amount_10023 = 15 * eth_price + 10 * usd_value_multiplier();
        assert!(
            record.per_hour_amounts() ==
            &vector[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                expected_notion_amount_10023,
            ],
            0,
        );
        assert_eq(record.total_amount(), expected_notion_amount_10023);

        // hour 1 (10024): $20
        clock.increment_for_testing(60 * 60 * 1000);
        // 2 btc = $20
        assert!(
            limiter.check_and_record_sending_transfer<BTC>(
                &treasury,
                &clock,
                route, 2 * treasury::decimal_multiplier<BTC>(&treasury),
            ),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        assert_eq(record.hour_head(), 10024);
        assert_eq(record.hour_tail(), 10001);
        let expected_notion_amount_10024 = 20 * usd_value_multiplier();
        assert!(
            record.per_hour_amounts() ==
            &vector[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                expected_notion_amount_10023,
                expected_notion_amount_10024,
            ],
            0,
        );
        assert_eq(record.total_amount(), expected_notion_amount_10023 + expected_notion_amount_10024);

        // Fast forward 22 hours, now hour 23 (10046): try to transfer $33 willf fail
        clock.increment_for_testing(60 * 60 * 1000 * 22);
        // fail
        // 65 usdt = $33
        assert!(
            !limiter.check_and_record_sending_transfer<USDT>(
                &treasury, &clock, route, 66 * 1_000_000,
            ),
            0,
        );
        // but window slid
        let record = limiter.transfer_records().get(&route);
        assert_eq(record.hour_head(), 10046);
        assert_eq(record.hour_tail(), 10023);
        assert!(
            record.per_hour_amounts() ==
            &vector[
                expected_notion_amount_10023, expected_notion_amount_10024,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            0,
        );
        assert_eq(record.total_amount(), expected_notion_amount_10023 + expected_notion_amount_10024);

        // hour 23 (10046): $32.5 deposit will succeed
        // 65 usdt = $32.5
        assert!(
            limiter.check_and_record_sending_transfer<USDT>(
                &treasury, &clock, route, 65 * 1_000_000,
            ),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        let expected_notion_amount_10046 = 325 * usd_value_multiplier() / 10;
        assert_eq(record.hour_head(), 10046);
        assert_eq(record.hour_tail(), 10023);
        assert!(
            record.per_hour_amounts() ==
            &vector[
                expected_notion_amount_10023,
                expected_notion_amount_10024,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                expected_notion_amount_10046,
            ],
            0,
        );
        assert_eq(record.total_amount(), expected_notion_amount_10023 + expected_notion_amount_10024 + expected_notion_amount_10046);

        // Hour 24 (10047), we can deposit $0.5 now
        clock.increment_for_testing(60 * 60 * 1000);
        // 1 usdt = $0.5
        assert!(
            limiter.check_and_record_sending_transfer<USDT>(&treasury, &clock, route, 1_000_000),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        let expected_notion_amount_10047 = 5 * usd_value_multiplier() / 10;
        assert_eq(record.hour_head(), 10047);
        assert_eq(record.hour_tail(), 10024);
        assert!(
            record.per_hour_amounts() ==
            &vector[
                expected_notion_amount_10024,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                expected_notion_amount_10046,
                expected_notion_amount_10047,
            ],
            0,
        );
        assert_eq(record.total_amount(), expected_notion_amount_10024 + expected_notion_amount_10046 + expected_notion_amount_10047);

        // Fast forward to Hour 30 (10053)
        clock.increment_for_testing(60 * 60 * 1000 * 6);
        // 1 usdc = $1
        assert!(
            limiter.check_and_record_sending_transfer<USDC>(
                &treasury,
                &clock,
                route,
                1 * treasury::decimal_multiplier<USDC>(&treasury),
            ),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        let expected_notion_amount_10053 = 1 * usd_value_multiplier();
        assert_eq(record.hour_head(), 10053);
        assert_eq(record.hour_tail(), 10030);
        assert!(
            record.per_hour_amounts() ==
            &vector[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                expected_notion_amount_10046,
                expected_notion_amount_10047,
                0, 0, 0, 0, 0,
                expected_notion_amount_10053,
            ],
            0,
        );
        assert_eq(record.total_amount(), expected_notion_amount_10046 + expected_notion_amount_10047 + expected_notion_amount_10053);

        // Fast forward to hour 130 (10153)
        clock.increment_for_testing(60 * 60 * 1000 * 100);
        // 1 usdc = $1
        assert!(
            limiter.check_and_record_sending_transfer<USDC>(
                &treasury,
                &clock,
                route,
                treasury::decimal_multiplier<USDC>(&treasury),
            ),
            0,
        );
        let record = limiter.transfer_records().get(&route);
        let expected_notion_amount_10153 = 1 * usd_value_multiplier();
        assert_eq(record.hour_head(), 10153);
        assert_eq(record.hour_tail(), 10130);
        assert!(
            record.per_hour_amounts() ==
            &vector[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                expected_notion_amount_10153,
            ],
            0,
        );
        assert_eq(record.total_amount(), expected_notion_amount_10153);

        destroy(limiter);
        destroy(treasury);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_update_route_limit() {
        // default routes, default notion values
        let mut limiter = new();
        assert_eq(
            limiter.transfer_limits()[
                &chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet())
            ],
            5_000_000 * usd_value_multiplier(),
        );

        assert_eq(
            limiter.transfer_limits()[
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ],
            max_transfer_limit(),
        );

        // shrink testnet limit
        update_route_limit(&mut limiter, &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet()), 1_000 * usd_value_multiplier());
        assert_eq(
            limiter.transfer_limits()[
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ],
            1_000 * usd_value_multiplier(),
        );
        // mainnet route does not change
        assert_eq(
            limiter.transfer_limits()[
                &chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet())
            ],
            5_000_000 * usd_value_multiplier(),
        );
        destroy(limiter);
    }

    #[test]
    fun test_update_route_limit_all_paths() {
        let mut limiter = new();
        // pick an existing route limit
        assert_eq(
            limiter.transfer_limits()[
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ],
            max_transfer_limit(),
        );
        let new_limit = 1_000 * usd_value_multiplier();
        update_route_limit(&mut limiter, &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet()), new_limit);
        assert_eq(
            limiter.transfer_limits()[
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ],
            new_limit,
        );

        // pick a new route limit
        update_route_limit(&mut limiter, &chain_ids::get_route(chain_ids::sui_testnet(), chain_ids::eth_sepolia()), new_limit);
        assert_eq(
            limiter.transfer_limits()[
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ],
            new_limit,
        );


        destroy(limiter);
    }

    #[test]
    fun test_update_asset_price() {
        // default routes, default notion values
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut treasury = treasury::mock_for_test(ctx);

        assert_eq(treasury.notional_value<BTC>(), (50_000 * usd_value_multiplier()));
        assert_eq(treasury.notional_value<ETH>(), (3_000 * usd_value_multiplier()));
        assert_eq(treasury.notional_value<USDC>(), (1 * usd_value_multiplier()));
        assert_eq(treasury.notional_value<USDT>(), (1 * usd_value_multiplier()));
        // change usdt price
        let id = treasury.token_id<USDT>();
        treasury.update_asset_notional_price(id, 11 * usd_value_multiplier() / 10);
        assert_eq(treasury.notional_value<USDT>(), (11 * usd_value_multiplier() / 10));
        // other prices do not change
        assert_eq(treasury.notional_value<BTC>(), (50_000 * usd_value_multiplier()));
        assert_eq(treasury.notional_value<ETH>(), (3_000 * usd_value_multiplier()));
        assert_eq(treasury.notional_value<USDC>(), (1 * usd_value_multiplier()));
        scenario.end();
        destroy(treasury);
    }
}
