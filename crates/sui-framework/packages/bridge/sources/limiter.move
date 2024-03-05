// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::limiter {

    use std::option;
    use std::vector;

    use sui::clock::{Self, Clock};
    use sui::event::emit;
    use sui::vec_map::{Self, VecMap};

    use bridge::btc::BTC;
    use bridge::chain_ids::{Self, BridgeRoute};
    use bridge::eth::ETH;
    use bridge::treasury;
    use bridge::usdc::USDC;
    use bridge::usdt::USDT;

    #[test_only]
    use sui::test_scenario;
    #[test_only]
    use sui::test_utils::{assert_eq, destroy};
    #[test_only]
    use bridge::btc;
    #[test_only]
    use bridge::eth;
    #[test_only]
    use bridge::usdc;
    #[test_only]
    use bridge::usdt;

    friend bridge::bridge;

    const ELimitNotFoundForRoute: u64 = 0;

    // TODO: U64::MAX, make this configurable?
    const MAX_TRANSFER_LIMIT: u64 = 18_446_744_073_709_551_615;

    const USD_VALUE_MULTIPLIER: u64 = 10000; // 4 DP accuracy

    struct TransferLimiter has store {
        transfer_limits: VecMap<BridgeRoute, u64>,
        // token id to USD notional value, 4 DP accuracy, so 10000 => 1USD
        notional_values: VecMap<u8, u64>,
        // Per hour transfer amount for each bridge route
        transfer_records: VecMap<BridgeRoute, TransferRecord>,
    }

    struct TransferRecord has store {
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
            notional_values: initial_notional_values(),
            transfer_records: vec_map::empty()
        }
    }

    struct UpdateRouteLimitEvent has copy, drop {
        sending_chain: u8,
        receiving_chain: u8,
        new_limit: u64,
    }

    struct UpdateAssetPriceEvent has copy, drop {
        token_id: u8,
        new_price: u64,
    }

    // Abort if the route limit is not found
    public fun get_route_limit(self: &TransferLimiter, route: &BridgeRoute): u64 {
        *vec_map::get(&self.transfer_limits, route)
    }

    // Abort if the token's notional price is not found
    public fun get_asset_notional_price(self: &TransferLimiter, token_id: &u8): u64 {
        *vec_map::get(&self.notional_values, token_id)
    }

    public(friend) fun update_route_limit(self: &mut TransferLimiter, route: &BridgeRoute, new_usd_limit: u64) {
        let receiving_chain = *chain_ids::route_destination(route);
        if (!vec_map::contains(&self.transfer_limits, route)) {
            vec_map::insert(&mut self.transfer_limits, *route, new_usd_limit);
        } else {
            let entry = vec_map::get_mut(&mut self.transfer_limits, route);
            *entry = new_usd_limit;
        };
        emit(UpdateRouteLimitEvent {
            sending_chain: *chain_ids::route_source(route),
            receiving_chain,
            new_limit: new_usd_limit,
        })
    }

    public(friend) fun update_asset_notional_price(self: &mut TransferLimiter, token_id: u8, new_usd_price: u64) {
        if (!vec_map::contains(&self.notional_values, &token_id)) {
            vec_map::insert(&mut self.notional_values, token_id, new_usd_price);
        } else {
            let entry = vec_map::get_mut(&mut self.notional_values, &token_id);
            *entry = new_usd_price;
        };
        emit(UpdateAssetPriceEvent {
            token_id,
            new_price: new_usd_price,
        })
    }

    // Current hour since unix epoch
    fun current_hour_since_epoch(clock: &Clock): u64 {
        clock::timestamp_ms(clock) / 3600000
    }

    public fun check_and_record_sending_transfer<T>(
        self: &mut TransferLimiter,
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
        let route_limit = vec_map::try_get(&self.transfer_limits, &route);
        assert!(option::is_some(&route_limit), ELimitNotFoundForRoute);
        let route_limit = option::destroy_some(route_limit);
        let route_limit_adjusted = (route_limit as u128) * (treasury::decimal_multiplier<T>() as u128);

        // Compute notional amount
        // Upcast to u128 to prevent overflow, to not miss out on small amounts.
        let notional_amount_with_token_multiplier = (*vec_map::get(&self.notional_values, &treasury::token_id<T>()) as u128) * (amount as u128);

        // Check if transfer amount exceed limit
        // Upscale them to the token's decimal.
        if ((record.total_amount as u128) * (treasury::decimal_multiplier<T>() as u128) + notional_amount_with_token_multiplier > route_limit_adjusted) {
            return false
        };

        // Now scale down to notional value
        let notional_amount = notional_amount_with_token_multiplier / (treasury::decimal_multiplier<T>() as u128);
        // Should be safe to downcast to u64 after dividing by the decimals
        let notional_amount = (notional_amount as u64);

        // Record transfer value
        let new_amount = vector::pop_back(&mut record.per_hour_amounts) + notional_amount;
        vector::push_back(&mut record.per_hour_amounts, new_amount);
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
            self.per_hour_amounts = vector::empty();
            self.total_amount = 0;
            self.hour_tail = target_tail;
            self.hour_head = target_tail;
            // Don't forget to insert this hour's record
            vector::push_back(&mut self.per_hour_amounts, 0);
        } else {
            // self.hour_head is within 24 hour range.
            // some items in `per_hour_amounts` are still valid, we remove stale hours.
            while (self.hour_tail < target_tail) {
                self.total_amount = self.total_amount - vector::remove(&mut self.per_hour_amounts, 0);
                self.hour_tail = self.hour_tail + 1;
            }
        };

        // Backfill from hour_head to current hour
        while (self.hour_head < current_hour_since_epoch) {
            vector::push_back(&mut self.per_hour_amounts, 0);
            self.hour_head = self.hour_head + 1;
        }
    }

    // It's tedious to list every pair, but it's safer to do so so we don't
    // accidentally turn off limiter for a new production route in the future.
    // Note limiter only takes effects on the receiving chain, so we only need to
    // specify routes from Ethereum to Sui.
    fun initial_transfer_limits(): VecMap<BridgeRoute, u64> {
        let transfer_limits = vec_map::empty();
        // 5M limit on Sui -> Ethereum mainnet
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet()),
            5_000_000 * USD_VALUE_MULTIPLIER
        );

        // MAX limit for testnet and devnet
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet()),
            MAX_TRANSFER_LIMIT
        );
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_devnet()),
            MAX_TRANSFER_LIMIT
        );
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_local_test()),
            MAX_TRANSFER_LIMIT
        );
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_local_test(), chain_ids::sui_testnet()),
            MAX_TRANSFER_LIMIT
        );
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_local_test(), chain_ids::sui_devnet()),
            MAX_TRANSFER_LIMIT
        );
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_local_test(), chain_ids::sui_local_test()),
            MAX_TRANSFER_LIMIT
        );
        transfer_limits
    }

    fun initial_notional_values(): VecMap<u8, u64> {
        let notional_values = vec_map::empty();
        vec_map::insert(&mut notional_values, treasury::token_id<BTC>(), 50_000 * USD_VALUE_MULTIPLIER);
        vec_map::insert(&mut notional_values, treasury::token_id<ETH>(), 3_000 * USD_VALUE_MULTIPLIER);
        vec_map::insert(&mut notional_values, treasury::token_id<USDC>(), 1 * USD_VALUE_MULTIPLIER);
        vec_map::insert(&mut notional_values, treasury::token_id<USDT>(), 1 * USD_VALUE_MULTIPLIER);
        notional_values
    }

    #[test]
    fun test_24_hours_windows() {
        let limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            notional_values: vec_map::empty(),
            transfer_records: vec_map::empty(),
        };

        let route = chain_ids::get_route(chain_ids::sui_devnet(), chain_ids::eth_sepolia());

        // Global transfer limit is 100M USD
        vec_map::insert(&mut limiter.transfer_limits, route, 100_000_000 * USD_VALUE_MULTIPLIER);
        // Notional price for ETH is 5 USD
        vec_map::insert(&mut limiter.notional_values, treasury::token_id<ETH>(), 5 * USD_VALUE_MULTIPLIER);

        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 1706288001377);

        // transfer 10000 ETH every hour, the totol should be 10000 * 5
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 10_000 * eth::multiplier()), 0);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 10000 * 5 * USD_VALUE_MULTIPLIER, 0);

        // transfer 1000 ETH every hour for 50 hours, the 24 hours totol should be 24000 * 10
        let i = 0;
        while (i < 50) {
            clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
            assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 1_000 * eth::multiplier()), 0);
            i = i + 1;
        };
        let record = vec_map::get(&limiter.transfer_records, &route);
        let expected_value = 24000 * 5 * USD_VALUE_MULTIPLIER;
        assert_eq(record.total_amount, expected_value);

        // transfer 1000 * i ETH every hour for 24 hours, the 24 hours totol should be 300 * 1000 * 5
        let i = 0;
        // At this point, every hour in past 24 hour has value $5000.
        // In each iteration, the old $5000 gets replaced with (i * 5000)
        while (i < 24) {
            clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
            assert!(
                check_and_record_sending_transfer<ETH>(
                    &mut limiter,
                    &clock,
                    route,
                    1_000 * eth::multiplier() * (i + 1)
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

        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_24_hours_windows_multiple_route() {
        let limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            notional_values: vec_map::empty(),
            transfer_records: vec_map::empty(),
        };

        let route = chain_ids::get_route(chain_ids::sui_devnet(), chain_ids::eth_sepolia());
        let route2 = chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_devnet());

        // Global transfer limit is 1M USD
        vec_map::insert(&mut limiter.transfer_limits, route, 1_000_000 * USD_VALUE_MULTIPLIER);
        vec_map::insert(&mut limiter.transfer_limits, route2, 500_000 * USD_VALUE_MULTIPLIER);
        // Notional price for ETH is 5 USD
        vec_map::insert(&mut limiter.notional_values, treasury::token_id<ETH>(), 5 * USD_VALUE_MULTIPLIER);

        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 1706288001377);

        // Transfer 10000 ETH on route 1
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 10_000 * eth::multiplier()), 0);
        // Transfer 50000 ETH on route 2
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route2, 50_000 * eth::multiplier()), 0);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 10000 * 5 * USD_VALUE_MULTIPLIER, 0);

        let record = vec_map::get(&limiter.transfer_records, &route2);
        assert!(record.total_amount == 50000 * 5 * USD_VALUE_MULTIPLIER, 0);

        destroy(limiter);

        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_exceed_limit() {
        let limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            notional_values: vec_map::empty(),
            transfer_records: vec_map::empty(),
        };

        let route = chain_ids::get_route(chain_ids::sui_devnet(), chain_ids::eth_sepolia());
        // Global transfer limit is 1M USD
        vec_map::insert(&mut limiter.transfer_limits, route, 1_000_000 * USD_VALUE_MULTIPLIER);
        // Notional price for ETH is 10 USD
        vec_map::insert(&mut limiter.notional_values, treasury::token_id<ETH>(), 10 * USD_VALUE_MULTIPLIER);

        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 1706288001377);

        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 90_000 * eth::multiplier()), 0);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 90000 * 10 * USD_VALUE_MULTIPLIER);

        clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 10_000 * eth::multiplier()), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 100000 * 10 * USD_VALUE_MULTIPLIER);

        // Tx should fail with a tiny amount because the limit is hit
        assert!(!check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 1), 0);
        assert!(!check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 90_000 * eth::multiplier()), 0);

        // Fast forward 23 hours, now the first 90k should be discarded
        clock::increment_for_testing(&mut clock, 60 * 60 * 1000 * 23);
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 90_000 * eth::multiplier()), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 100000 * 10 * USD_VALUE_MULTIPLIER);

        // But now limit is hit again
        assert!(!check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 1), 0);
        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 100000 * 10 * USD_VALUE_MULTIPLIER);

        destroy(limiter);

        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_initial_limiter_setting() {
        // default routes, default notion values
        let limiter = new();
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<BTC>()), (50_000 * USD_VALUE_MULTIPLIER));
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<ETH>()), (3_000 * USD_VALUE_MULTIPLIER));
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<USDC>()), (1 * USD_VALUE_MULTIPLIER));
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<USDT>()), (1 * USD_VALUE_MULTIPLIER));

        assert_eq(
            *vec_map::get(
                &limiter.transfer_limits, 
                &chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet())
            ),
            5_000_000 * USD_VALUE_MULTIPLIER,
        );

        assert!(vec_map::is_empty(&limiter.transfer_records), 0);
        destroy(limiter);
    }

    #[test]
    #[expected_failure(abort_code = ELimitNotFoundForRoute)]
    fun test_limiter_does_not_limit_receiving_transfers() {
        let limiter = new();

        let route = chain_ids::get_route(chain_ids::sui_mainnet(), chain_ids::eth_mainnet());
        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 1706288001377);
        // We don't limit sui -> eth transfers. This aborts with `ELimitNotFoundForRoute`
        check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 1 * eth::multiplier());
        destroy(limiter);
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_limiter_basic_op() {
        // In this test we use very simple number for easier calculation.
        let limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            notional_values: vec_map::empty(),
            transfer_records: vec_map::empty(),
        };
        let route = chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet());
        // Global transfer limit is 100 USD
        vec_map::insert(&mut limiter.transfer_limits, route, 100 * USD_VALUE_MULTIPLIER);
        // BTC: $10, ETH: $2.5, USDC: $1, USDT: $0.5
        vec_map::insert(&mut limiter.notional_values, treasury::token_id<BTC>(), 10 * USD_VALUE_MULTIPLIER);
        vec_map::insert(&mut limiter.notional_values, treasury::token_id<ETH>(), 25000);
        vec_map::insert(&mut limiter.notional_values, treasury::token_id<USDC>(), 1 * USD_VALUE_MULTIPLIER);
        vec_map::insert(&mut limiter.notional_values, treasury::token_id<USDT>(), 5000);

        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 36082800000); // hour 10023

        // hour 0 (10023): $15 * 2.5 = $37.5
        // 15 eth = $37.5
        assert!(check_and_record_sending_transfer<ETH>(&mut limiter, &clock, route, 15 * eth::multiplier()), 0);
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
        assert!(check_and_record_sending_transfer<USDC>(&mut limiter, &clock, route, 10 * usdc::multiplier()), 0);
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
        clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
        // 2 btc = $20
        assert!(check_and_record_sending_transfer<BTC>(&mut limiter, &clock, route, 2 * btc::multiplier()), 0);
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
        clock::increment_for_testing(&mut clock, 60 * 60 * 1000 * 22);
        // fail
        // 65 usdt = $33
        assert!(!check_and_record_sending_transfer<USDT>(&mut limiter, &clock, route, 66 * usdt::multiplier()), 0);
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
        assert!(check_and_record_sending_transfer<USDT>(&mut limiter, &clock, route, 65 * usdt::multiplier()), 0);
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
        clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
        // 1 usdt = $0.5
        assert!(check_and_record_sending_transfer<USDT>(&mut limiter, &clock, route, 1 * usdt::multiplier()), 0);
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
        clock::increment_for_testing(&mut clock, 60 * 60 * 1000 * 6);
        // 1 usdc = $1
        assert!(check_and_record_sending_transfer<USDC>(&mut limiter, &clock, route, 1 * usdc::multiplier()), 0);
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
        clock::increment_for_testing(&mut clock, 60 * 60 * 1000 * 100);
        // 1 usdc = $1
        assert!(check_and_record_sending_transfer<USDC>(&mut limiter, &clock, route, 1 * usdc::multiplier()), 0);
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
        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_update_route_limit() {
        // default routes, default notion values
        let limiter = new();
        assert_eq(
            *vec_map::get(
                &limiter.transfer_limits, 
                &chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet())
            ),
            5_000_000 * USD_VALUE_MULTIPLIER,
        );

        assert_eq(
            *vec_map::get(
                &limiter.transfer_limits, 
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ),
            MAX_TRANSFER_LIMIT,
        );

        // shrink testnet limit
        update_route_limit(&mut limiter, &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet()), 1_000 * USD_VALUE_MULTIPLIER);
        assert_eq(
            *vec_map::get(
                &limiter.transfer_limits, 
                &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())
            ),
            1_000 * USD_VALUE_MULTIPLIER,
        );
        // mainnet route does not change
        assert_eq(
            *vec_map::get(
                &limiter.transfer_limits, 
                &chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet())
            ),
            5_000_000 * USD_VALUE_MULTIPLIER,
        );
        destroy(limiter);
    }

    #[test]
    fun test_update_asset_price() {
        // default routes, default notion values
        let limiter = new();
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<BTC>()), (50_000 * USD_VALUE_MULTIPLIER));
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<ETH>()), (3_000 * USD_VALUE_MULTIPLIER));
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<USDC>()), (1 * USD_VALUE_MULTIPLIER));
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<USDT>()), (1 * USD_VALUE_MULTIPLIER));

        // change usdt price
        update_asset_notional_price(&mut limiter, treasury::token_id<USDT>(), 11 * USD_VALUE_MULTIPLIER / 10);
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<USDT>()), (11 * USD_VALUE_MULTIPLIER / 10));
        // other prices do not change
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<BTC>()), (50_000 * USD_VALUE_MULTIPLIER));
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<ETH>()), (3_000 * USD_VALUE_MULTIPLIER));
        assert_eq(*vec_map::get(&limiter.notional_values, &treasury::token_id<USDC>()), (1 * USD_VALUE_MULTIPLIER));
        destroy(limiter);
    }

}
