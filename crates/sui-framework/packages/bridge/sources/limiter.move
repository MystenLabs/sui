// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::limiter {
    use sui::clock::{Self, Clock};
    use sui::event::emit;
    use sui::vec_map::{Self, VecMap};

    use bridge::chain_ids::{Self, BridgeRoute};
    use bridge::treasury::BridgeTreasury;

    #[test_only]
    use sui::test_scenario;
    #[test_only]
    use sui::test_utils::{assert_eq, destroy};
    #[test_only]
    use bridge::treasury::{Self, BTC, ETH, USDC, USDT};

    const ELimitNotFoundForRoute: u64 = 0;

    // TODO: U64::MAX, make this configurable?
    const MAX_TRANSFER_LIMIT: u64 = 18_446_744_073_709_551_615;

    const USD_VALUE_MULTIPLIER: u64 = 10000; // 4 DP accuracy

    public struct TransferLimiter has store {
        transfer_limits: VecMap<BridgeRoute, u64>,
        // Per hour transfer amount for each bridge route
        transfer_records: VecMap<BridgeRoute, TransferRecord>,
    }

    public struct TransferRecord has store {
        hour_head: u64,
        hour_tail: u64,
        per_hour_amounts: vector<u64>,
        // total amount in USD, 4 DP accuracy, so 10000 => 1USD
        total_amount: u64
    }

    public fun new(): TransferLimiter {
        // hardcoded limit for bridge genesis
        TransferLimiter {
            transfer_limits: initial_transfer_limits(),
            transfer_records: vec_map::empty()
        }
    }

    public struct UpdateRouteLimitEvent has copy, drop {
        sending_chain: u8,
        receiving_chain: u8,
        new_limit: u64,
    }

    // Abort if the route limit is not found
    public fun get_route_limit(self: &TransferLimiter, route: &BridgeRoute): u64 {
        self.transfer_limits[route]
    }

    public(package) fun update_route_limit(
        self: &mut TransferLimiter,
        route: &BridgeRoute,
        new_usd_limit: u64
    ) {
        let receiving_chain = *route.destination();

        if (!self.transfer_limits.contains(route)) {
            self.transfer_limits.insert(*route, new_usd_limit);
        } else {
            *&mut self.transfer_limits[route] = new_usd_limit;
        };

        emit(UpdateRouteLimitEvent {
            sending_chain: *route.source(),
            receiving_chain,
            new_limit: new_usd_limit,
        })
    }

    // Current hour since unix epoch
    fun current_hour_since_epoch(clock: &Clock): u64 {
        clock::timestamp_ms(clock) / 3600000
    }

    public fun check_and_record_sending_transfer<T>(
        self: &mut TransferLimiter,
        treasury: &BridgeTreasury,
        clock: &Clock,
        route: BridgeRoute,
        amount: u64
    ): bool {
        // Create record for route if not exists
        if (!vec_map::contains(&self.transfer_records, &route)) {
            vec_map::insert(&mut self.transfer_records, route, TransferRecord {
                hour_head: 0,
                hour_tail: 0,
                per_hour_amounts: vector[],
                total_amount: 0
            })
        };
        let record = vec_map::get_mut(&mut self.transfer_records, &route);
        let current_hour_since_epoch = current_hour_since_epoch(clock);

        adjust_transfer_records(record, current_hour_since_epoch);

        // Get limit for the route
        let route_limit = self.transfer_limits.try_get(&route);
        assert!(route_limit.is_some(), ELimitNotFoundForRoute);
        let route_limit = route_limit.destroy_some();
        let route_limit_adjusted = (route_limit as u128) * (treasury.decimal_multiplier<T>() as u128);

        // Compute notional amount
        // Upcast to u128 to prevent overflow, to not miss out on small amounts.
        let value = (treasury.notional_value<T>() as u128);
        let notional_amount_with_token_multiplier = value * (amount as u128);

        // Check if transfer amount exceed limit
        // Upscale them to the token's decimal.
        if ((record.total_amount as u128) * (treasury.decimal_multiplier<T>() as u128) + notional_amount_with_token_multiplier > route_limit_adjusted) {
            return false
        };

        // Now scale down to notional value
        let notional_amount = notional_amount_with_token_multiplier / (treasury.decimal_multiplier<T>() as u128);
        // Should be safe to downcast to u64 after dividing by the decimals
        let notional_amount = (notional_amount as u64);

        // Record transfer value
        let new_amount = record.per_hour_amounts.pop_back() + notional_amount;
        record.per_hour_amounts.push_back(new_amount);
        record.total_amount = record.total_amount + notional_amount;
        true
    }

    fun adjust_transfer_records(self: &mut TransferRecord, current_hour_since_epoch: u64) {
        if (self.hour_head == current_hour_since_epoch) {
            return // nothing to backfill
        };

        let target_tail = current_hour_since_epoch - 23;

        // If `hour_head` is even older than 24 hours ago, it means all items in
        // `per_hour_amounts` are to be evicted.
        if (self.hour_head < target_tail) {
            self.per_hour_amounts = vector[];
            self.total_amount = 0;
            self.hour_tail = target_tail;
            self.hour_head = target_tail;
            // Don't forget to insert this hour's record
            self.per_hour_amounts.push_back(0);
        } else {
            // self.hour_head is within 24 hour range.
            // some items in `per_hour_amounts` are still valid, we remove stale hours.
            while (self.hour_tail < target_tail) {
                self.total_amount = self.total_amount - self.per_hour_amounts.remove(0);
                self.hour_tail = self.hour_tail + 1;
            }
        };

        // Backfill from hour_head to current hour
        while (self.hour_head < current_hour_since_epoch) {
            self.per_hour_amounts.push_back(0);
            self.hour_head = self.hour_head + 1;
        }
    }

    // It's tedious to list every pair, but it's safer to do so so we don't
    // accidentally turn off limiter for a new production route in the future.
    // Note limiter only takes effects on the receiving chain, so we only need to
    // specify routes from Ethereum to Sui.
    fun initial_transfer_limits(): VecMap<BridgeRoute, u64> {
        let mut transfer_limits = vec_map::empty();
        // 5M limit on Sui -> Ethereum mainnet
        transfer_limits.insert(
            chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet()),
            5_000_000 * USD_VALUE_MULTIPLIER
        );

        // MAX limit for testnet and devnet
        transfer_limits.insert(
            chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet()),
            MAX_TRANSFER_LIMIT
        );

        transfer_limits.insert(
            chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_devnet()),
            MAX_TRANSFER_LIMIT
        );

        transfer_limits.insert(
            chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_local_test()),
            MAX_TRANSFER_LIMIT
        );

        transfer_limits.insert(
            chain_ids::get_route(chain_ids::eth_local_test(), chain_ids::sui_testnet()),
            MAX_TRANSFER_LIMIT
        );

        transfer_limits.insert(
            chain_ids::get_route(chain_ids::eth_local_test(), chain_ids::sui_devnet()),
            MAX_TRANSFER_LIMIT
        );

        transfer_limits.insert(
            chain_ids::get_route(chain_ids::eth_local_test(), chain_ids::sui_local_test()),
            MAX_TRANSFER_LIMIT
        );

        transfer_limits
    }

    #[test]
    fun test_24_hours_windows() {
        let mut limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            transfer_records: vec_map::empty(),
        };

        let route = chain_ids::get_route(chain_ids::sui_devnet(), chain_ids::eth_sepolia());

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut treasury = treasury::mock_for_test(ctx);

        // Global transfer limit is 100M USD
        limiter.transfer_limits.insert(route, 100_000_000 * USD_VALUE_MULTIPLIER);
        // Notional price for ETH is 5 USD
        let id = treasury::token_id<ETH>(&treasury);
        treasury.update_asset_notional_price(id, 5 * USD_VALUE_MULTIPLIER);

        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(1706288001377);

        // transfer 10000 ETH every hour, the totol should be 10000 * 5
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 10_000 * treasury.decimal_multiplier<ETH>()), 0);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 10000 * 5 * USD_VALUE_MULTIPLIER, 0);

        // transfer 1000 ETH every hour for 50 hours, the 24 hours totol should be 24000 * 10
        let mut i = 0;
        while (i < 50) {
            clock.increment_for_testing(60 * 60 * 1000);
            assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 1_000 * treasury.decimal_multiplier<ETH>()), 0);
            i = i + 1;
        };
        let record = vec_map::get(&limiter.transfer_records, &route);
        let mut expected_value = 24000 * 5 * USD_VALUE_MULTIPLIER;
        assert_eq(record.total_amount, expected_value);

        // transfer 1000 * i ETH every hour for 24 hours, the 24 hours totol should be 300 * 1000 * 5
        let mut i = 0;
        // At this point, every hour in past 24 hour has value $5000.
        // In each iteration, the old $5000 gets replaced with (i * 5000)
        while (i < 24) {
            clock.increment_for_testing(60 * 60 * 1000);
            assert!(
                check_and_record_sending_transfer<ETH>(
                    &mut limiter,
                    &treasury,
                    &clock,
                    route,
                    1_000 * treasury.decimal_multiplier<ETH>() * (i + 1)
                ),
                0
            );

            let record = vec_map::get(&limiter.transfer_records, &route);

            expected_value = expected_value + 1000 * 5 * i * USD_VALUE_MULTIPLIER;
            assert_eq(record.total_amount, expected_value);
            i = i + 1;
        };

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 300 * 1000 * 5 * USD_VALUE_MULTIPLIER);

        destroy(limiter);
        destroy(treasury);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_24_hours_windows_multiple_route() {
        let mut limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            transfer_records: vec_map::empty(),
        };

        let route = chain_ids::get_route(chain_ids::sui_devnet(), chain_ids::eth_sepolia());
        let route2 = chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_devnet());

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut treasury = treasury::mock_for_test(ctx);

        // Global transfer limit is 1M USD
        limiter.transfer_limits.insert(route, 1_000_000 * USD_VALUE_MULTIPLIER);
        limiter.transfer_limits.insert(route2, 500_000 * USD_VALUE_MULTIPLIER);
        // Notional price for ETH is 5 USD
        let id = treasury::token_id<ETH>(&treasury);
        treasury.update_asset_notional_price(id, 5 * USD_VALUE_MULTIPLIER);

        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(1706288001377);

        // Transfer 10000 ETH on route 1
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 10_000 * treasury::decimal_multiplier<ETH>(&treasury)), 0);
        // Transfer 50000 ETH on route 2
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route2, 50_000 * treasury::decimal_multiplier<ETH>(&treasury)), 0);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 10000 * 5 * USD_VALUE_MULTIPLIER, 0);

        let record = vec_map::get(&limiter.transfer_records, &route2);
        assert!(record.total_amount == 50000 * 5 * USD_VALUE_MULTIPLIER, 0);

        destroy(limiter);
        destroy(treasury);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_exceed_limit() {
        let mut limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            transfer_records: vec_map::empty(),
        };

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut treasury = treasury::mock_for_test(ctx);

        let route = chain_ids::get_route(chain_ids::sui_devnet(), chain_ids::eth_sepolia());
        // Global transfer limit is 1M USD
        vec_map::insert(&mut limiter.transfer_limits, route, 1_000_000 * USD_VALUE_MULTIPLIER);
        // Notional price for ETH is 10 USD
        let id = treasury::token_id<ETH>(&treasury);
        treasury.update_asset_notional_price(id, 10 * USD_VALUE_MULTIPLIER);

        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(1706288001377);

        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 90_000 * treasury::decimal_multiplier<ETH>(&treasury)), 0);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 90000 * 10 * USD_VALUE_MULTIPLIER);

        clock.increment_for_testing(60 * 60 * 1000);
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 10_000 * treasury::decimal_multiplier<ETH>(&treasury)), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 100000 * 10 * USD_VALUE_MULTIPLIER);

        // Tx should fail with a tiny amount because the limit is hit
        assert!(!check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 1), 0);
        assert!(!check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 90_000 * treasury::decimal_multiplier<ETH>(&treasury)), 0);

        // Fast forward 23 hours, now the first 90k should be discarded
        clock.increment_for_testing(60 * 60 * 1000 * 23);
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 90_000 * treasury::decimal_multiplier<ETH>(&treasury)), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 100000 * 10 * USD_VALUE_MULTIPLIER);

        // But now limit is hit again
        assert!(!check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 1), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 100000 * 10 * USD_VALUE_MULTIPLIER);

        destroy(limiter);
        destroy(treasury);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = ELimitNotFoundForRoute)]
    fun test_limiter_does_not_limit_receiving_transfers() {
        let mut limiter = new();

        let route = chain_ids::get_route(chain_ids::sui_mainnet(), chain_ids::eth_mainnet());
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = scenario.ctx();
        let treasury = treasury::mock_for_test(ctx);
        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(1706288001377);
        // We don't limit sui -> eth transfers. This aborts with `ELimitNotFoundForRoute`
        check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 1 * treasury::decimal_multiplier<ETH>(&treasury));
        destroy(limiter);
        destroy(treasury);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_limiter_basic_op() {
        // In this test we use very simple number for easier calculation.
        let mut limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            transfer_records: vec_map::empty(),
        };

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut treasury = treasury::mock_for_test(ctx);

        let route = chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet());
        // Global transfer limit is 100 USD
        vec_map::insert(&mut limiter.transfer_limits, route, 100 * USD_VALUE_MULTIPLIER);
        // BTC: $10, ETH: $2.5, USDC: $1, USDT: $0.5
        let id = treasury::token_id<BTC>(&treasury);
        treasury.update_asset_notional_price(id, 10 * USD_VALUE_MULTIPLIER);
        let id = treasury::token_id<ETH>(&treasury);
        treasury.update_asset_notional_price(id, 25000);
        let id = treasury::token_id<USDC>(&treasury);
        treasury.update_asset_notional_price(id, 1 * USD_VALUE_MULTIPLIER);
        let id = treasury::token_id<USDT>(&treasury);
        treasury.update_asset_notional_price(id, 5000);

        let mut clock = clock::create_for_testing(ctx);
        clock.set_for_testing(36082800000); // hour 10023

        // hour 0 (10023): $15 * 2.5 = $37.5
        // 15 eth = $37.5
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &treasury, &clock, route, 15 * treasury::decimal_multiplier<ETH>(&treasury)), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.hour_head, 10023);
        assert_eq(record.hour_tail, 10000);
        assert_eq(
            record.per_hour_amounts,
            vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15 * 25000]
        );
        assert_eq(record.total_amount, 15 * 25000);

        // hour 0 (10023): $37.5 + $10 = $47.5
        // 10 uddc = $10
        assert!(check_and_record_sending_transfer<USDC>(&mut limiter, &treasury, &clock, route, 10 * treasury::decimal_multiplier<USDC>(&treasury)), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.hour_head, 10023);
        assert_eq(record.hour_tail, 10000);
        let expected_notion_amount_10023 = 15 * 25000 + 10 * USD_VALUE_MULTIPLIER;
        assert_eq(
            record.per_hour_amounts,
            vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, expected_notion_amount_10023]
        );
        assert_eq(record.total_amount, expected_notion_amount_10023);

        // hour 1 (10024): $20
        clock.increment_for_testing(60 * 60 * 1000);
        // 2 btc = $20
        assert!(check_and_record_sending_transfer<BTC>(&mut limiter, &treasury, &clock, route, 2 * treasury::decimal_multiplier<BTC>(&treasury)), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.hour_head, 10024);
        assert_eq(record.hour_tail, 10001);
        let expected_notion_amount_10024 = 20 * USD_VALUE_MULTIPLIER;
        assert_eq(
            record.per_hour_amounts,
            vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, expected_notion_amount_10023, expected_notion_amount_10024]
        );
        assert_eq(record.total_amount, expected_notion_amount_10023 + expected_notion_amount_10024);

        // Fast forward 22 hours, now hour 23 (10046): try to transfer $33 willf fail
        clock.increment_for_testing(60 * 60 * 1000 * 22);
        // fail
        // 65 usdt = $33
        assert!(!check_and_record_sending_transfer<USDT>(&mut limiter, &treasury, &clock, route, 66 * 1_000_000), 0);
        // but window slided
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.hour_head, 10046);
        assert_eq(record.hour_tail, 10023);
        assert_eq(
            record.per_hour_amounts,
            vector[expected_notion_amount_10023, expected_notion_amount_10024, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq(record.total_amount, expected_notion_amount_10023 + expected_notion_amount_10024);

        // hour 23 (10046): $32.5 deposit will succeed
        // 65 usdt = $32.5
        assert!(check_and_record_sending_transfer<USDT>(&mut limiter, &treasury, &clock, route, 65 * 1_000_000), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        let expected_notion_amount_10046 = 325 * USD_VALUE_MULTIPLIER / 10;
        assert_eq(record.hour_head, 10046);
        assert_eq(record.hour_tail, 10023);
        assert_eq(
            record.per_hour_amounts,
            vector[expected_notion_amount_10023, expected_notion_amount_10024, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, expected_notion_amount_10046]
        );
        assert_eq(record.total_amount, expected_notion_amount_10023 + expected_notion_amount_10024 + expected_notion_amount_10046);

        // Hour 24 (10047), we can deposit $0.5 now
        clock.increment_for_testing(60 * 60 * 1000);
        // 1 usdt = $0.5
        assert!(check_and_record_sending_transfer<USDT>(&mut limiter, &treasury, &clock, route, 1_000_000), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        let expected_notion_amount_10047 = 5 * USD_VALUE_MULTIPLIER / 10;
        assert_eq(record.hour_head, 10047);
        assert_eq(record.hour_tail, 10024);
        assert_eq(
            record.per_hour_amounts,
            vector[expected_notion_amount_10024, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, expected_notion_amount_10046, expected_notion_amount_10047]
        );
        assert_eq(record.total_amount, expected_notion_amount_10024 + expected_notion_amount_10046 + expected_notion_amount_10047);

        // Fast forward to Hour 30 (10053)
        clock.increment_for_testing(60 * 60 * 1000 * 6);
        // 1 usdc = $1
        assert!(check_and_record_sending_transfer<USDC>(&mut limiter, &treasury, &clock, route, 1 * treasury::decimal_multiplier<USDC>(&treasury)), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        let expected_notion_amount_10053 = 1 * USD_VALUE_MULTIPLIER;
        assert_eq(record.hour_head, 10053);
        assert_eq(record.hour_tail, 10030);
        assert_eq(
            record.per_hour_amounts,
            vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, expected_notion_amount_10046, expected_notion_amount_10047, 0, 0, 0, 0, 0, expected_notion_amount_10053]
        );
        assert_eq(record.total_amount, expected_notion_amount_10046 + expected_notion_amount_10047 + expected_notion_amount_10053);

        // Fast forward to hour 130 (10153)
        clock.increment_for_testing(60 * 60 * 1000 * 100);
        // 1 usdc = $1
        assert!(check_and_record_sending_transfer<USDC>(&mut limiter, &treasury, &clock, route, treasury::decimal_multiplier<USDC>(&treasury)), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        let expected_notion_amount_10153 = 1 * USD_VALUE_MULTIPLIER;
        assert_eq(record.hour_head, 10153);
        assert_eq(record.hour_tail, 10130);
        assert_eq(
            record.per_hour_amounts,
            vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, expected_notion_amount_10153]
        );
        assert_eq(record.total_amount, expected_notion_amount_10153);

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
            limiter.transfer_limits[
                &chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet())
            ],
            5_000_000 * USD_VALUE_MULTIPLIER,
        );

        assert_eq(
            limiter.transfer_limits[
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ],
            MAX_TRANSFER_LIMIT,
        );

        // shrink testnet limit
        update_route_limit(&mut limiter, &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet()), 1_000 * USD_VALUE_MULTIPLIER);
        assert_eq(
            limiter.transfer_limits[
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ],
            1_000 * USD_VALUE_MULTIPLIER,
        );
        // mainnet route does not change
        assert_eq(
            limiter.transfer_limits[
                &chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet())
            ],
            5_000_000 * USD_VALUE_MULTIPLIER,
        );
        destroy(limiter);
    }

    #[test]
    fun test_update_route_limit_all_paths() {
        let mut limiter = new();
        // pick an existing route limit
        assert_eq(
            limiter.transfer_limits[
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ],
            MAX_TRANSFER_LIMIT,
        );
        let new_limit = 1_000 * USD_VALUE_MULTIPLIER;
        update_route_limit(&mut limiter, &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet()), new_limit);
        assert_eq(
            limiter.transfer_limits[
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ],
            new_limit,
        );
        
        // pick a new route limit
        update_route_limit(&mut limiter, &chain_ids::get_route(chain_ids::sui_testnet(), chain_ids::eth_sepolia()), new_limit);
        assert_eq(
            limiter.transfer_limits[
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

        assert_eq(treasury.notional_value<BTC>(), (50_000 * USD_VALUE_MULTIPLIER));
        assert_eq(treasury.notional_value<ETH>(), (3_000 * USD_VALUE_MULTIPLIER));
        assert_eq(treasury.notional_value<USDC>(), (1 * USD_VALUE_MULTIPLIER));
        assert_eq(treasury.notional_value<USDT>(), (1 * USD_VALUE_MULTIPLIER));
        // change usdt price
        let id = treasury.token_id<USDT>();
        treasury.update_asset_notional_price(id, 11 * USD_VALUE_MULTIPLIER / 10);
        assert_eq(treasury.notional_value<USDT>(), (11 * USD_VALUE_MULTIPLIER / 10));
        // other prices do not change
        assert_eq(treasury.notional_value<BTC>(), (50_000 * USD_VALUE_MULTIPLIER));
        assert_eq(treasury.notional_value<ETH>(), (3_000 * USD_VALUE_MULTIPLIER));
        assert_eq(treasury.notional_value<USDC>(), (1 * USD_VALUE_MULTIPLIER));
        scenario.end();
        destroy(treasury);
    }

}
