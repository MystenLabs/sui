// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::limiter {

    use std::option;
    use std::type_name;
    use std::type_name::TypeName;
    use sui::clock;
    use sui::clock::Clock;
    use sui::math::pow;
    use sui::vec_map;
    use sui::vec_map::{VecMap};
    use bridge::chain_ids;
    use bridge::sized_vector;
    use bridge::sized_vector::SizedVector;
    use bridge::treasury;
    use bridge::chain_ids::BridgeRoute;

    #[test_only]
    use sui::test_scenario;
    #[test_only]
    use sui::test_utils::{destroy, assert_eq};
    #[test_only]
    use bridge::eth::ETH;

    // TODO: Make this configurable?
    const DEFAULT_TRANSFER_LIMIT: u64 = 0;
    const MAX_TRANSFER_LIMIT: u64 = 18_446_744_073_709_551_615;
    const WINDOW_SIZE: u64 = 24;

    struct TransferLimiter has store {
        transfer_limits: VecMap<BridgeRoute, u64>,
        // USD notional value, 4 DP accuracy
        notional_values: VecMap<TypeName, u64>,
        // Per hour transfer amount for each bridge route
        transfer_records: VecMap<BridgeRoute, TransferRecord>,
    }

    struct TransferRecord has store {
        last_recorded_hour: u64,
        per_hour_amounts: SizedVector<u64>,
        // total amount in USD, 4 DP accuracy
        total_amount: u64
    }

    public fun new(): TransferLimiter {
        // hardcoded limit for bridge genesis
        let transfer_limits = vec_map::empty();
        // 5M limit on SUI to ETH mainnet with 4DP accuracy
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet()),
            5_000_000 * 10_000
        );
        // MAX limit for testnet and devnet
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_devnet()),
            MAX_TRANSFER_LIMIT
        );
        vec_map::insert(
            &mut transfer_limits,
            chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_devnet()),
            MAX_TRANSFER_LIMIT
        );
        TransferLimiter {
            transfer_limits,
            notional_values: vec_map::empty(),
            transfer_records: vec_map::empty()
        }
    }

    // Current hour since unix epoch
    fun current_hour_since_epoch(clock: &Clock): u64 {
        clock::timestamp_ms(clock) / 3600000
    }

    public fun check_and_record_transfer<T>(
        clock: & Clock,
        self: &mut TransferLimiter,
        route: BridgeRoute,
        amount: u64
    ): bool {
        // Create record for route if not exists
        if (!vec_map::contains(&self.transfer_records, &route)) {
            vec_map::insert(&mut self.transfer_records, route, TransferRecord {
                last_recorded_hour: 0,
                per_hour_amounts: sized_vector::new(WINDOW_SIZE),
                total_amount: 0
            })
        };
        let record = vec_map::get_mut(&mut self.transfer_records, &route);
        let current_hour_since_epoch = current_hour_since_epoch(clock);

        // First clean up old transfer records
        cleanup_records(record, current_hour_since_epoch);

        // Get limit for the route
        let route_limit = option::destroy_with_default(
            vec_map::try_get(&self.transfer_limits, &route),
            DEFAULT_TRANSFER_LIMIT
        );

        // Compute notional amount
        let coin_type = type_name::get<T>();
        // Upcast to u128 to prevent overflow
        let notional_amount = (*vec_map::get(&self.notional_values, &coin_type) as u128) * (amount as u128);
        let notional_amount = notional_amount / (pow(10, treasury::token_decimals<T>()) as u128);
        // Should be safe to downcast to u64 after dividing by the decimals
        let notional_amount = (notional_amount as u64);

        // Check if transfer amount exceed limit
        if (record.total_amount + notional_amount > route_limit) {
            return false
        };

        // Record transfer value
        let new_amount = sized_vector::pop_back(&mut record.per_hour_amounts) + notional_amount;
        let poped = sized_vector::push_back(&mut record.per_hour_amounts, new_amount);
        option::destroy_none(poped);
        record.total_amount = record.total_amount + notional_amount;
        return true
    }

    fun cleanup_records(self: &mut TransferRecord, current_hour_since_epoch: u64) {
        if (self.last_recorded_hour == current_hour_since_epoch) {
            return // nothing to cleanup
        };

        // clean up stale records
        // calculate number of hours to pop, max 24 hours.
        let i = current_hour_since_epoch - self.last_recorded_hour;
        if (i > WINDOW_SIZE) {
            i = WINDOW_SIZE
        };
        while (i > 0) {
            let poped = sized_vector::push_back(&mut self.per_hour_amounts, 0);
            self.total_amount = self.total_amount - option::destroy_with_default(poped, 0);
            i = i - 1;
        };
        self.last_recorded_hour = current_hour_since_epoch
    }

    #[test]
    fun test_24_hours_windows() {
        let limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            notional_values: vec_map::empty(),
            // Per hour transfer amount for each bridge route
            transfer_records: vec_map::empty(),
        };

        let route = chain_ids::get_route(chain_ids::sui_devnet(), chain_ids::eth_sepolia());

        // Global transfer limit is 1000M USD
        vec_map::insert(&mut limiter.transfer_limits, route, 1_000_000_000);
        // Notional price for ETH is 5 USD
        vec_map::insert(&mut limiter.notional_values, type_name::get<ETH>(), 5);

        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 1706288001377);

        // transfer 10000 ETH every hour, the totol should be 10000 * 5
        check_and_record_transfer<ETH>(&clock, &mut limiter, route, 1_000_000_000_000);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 10000 * 5, 0);

        // transfer 1000 * i ETH every hour for 24 hours, the 24 hours totol should be 300 * 1000 * 5
        let i = 0;
        while (i < 24) {
            clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
            check_and_record_transfer<ETH>(&clock, &mut limiter, route, 100_000_000_000 * (i + 1));
            i = i + 1;
        };

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert_eq(record.total_amount, 300 * 1000 * 5);

        destroy(limiter);

        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_24_hours_windows_multiple_route() {
        let limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            notional_values: vec_map::empty(),
            // Per hour transfer amount for each bridge route
            transfer_records: vec_map::empty(),
        };

        let route = chain_ids::get_route(chain_ids::sui_devnet(), chain_ids::eth_sepolia());
        let route2 = chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_devnet());

        // Global transfer limit is 1M USD
        vec_map::insert(&mut limiter.transfer_limits, route, 1_000_000);
        vec_map::insert(&mut limiter.transfer_limits, route2, 500_000);
        // Notional price for ETH is 5 USD
        vec_map::insert(&mut limiter.notional_values, type_name::get<ETH>(), 5);

        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 1706288001377);

        // Transfer 10000 ETH (8 Decemal places) on route 1
        check_and_record_transfer<ETH>(&clock, &mut limiter, route, 1_000_000_000_000);
        // Transfer 50000 ETH (8 Decemal places) on route 2
        check_and_record_transfer<ETH>(&clock, &mut limiter, route2, 5_000_000_000_000);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 10000 * 5, 0);

        let record = vec_map::get(&limiter.transfer_records, &route2);
        assert!(record.total_amount == 50000 * 5, 0);

        destroy(limiter);

        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_exceed_limit() {
        let limiter = TransferLimiter {
            transfer_limits: vec_map::empty(),
            notional_values: vec_map::empty(),
            // Per hour transfer amount for each bridge route
            transfer_records: vec_map::empty(),
        };

        let route = chain_ids::get_route(chain_ids::sui_devnet(), chain_ids::eth_sepolia());
        // Global transfer limit is 1M USD
        vec_map::insert(&mut limiter.transfer_limits, route, 1_000_000);
        // Notional price for ETH is 10 USD
        vec_map::insert(&mut limiter.notional_values, type_name::get<ETH>(), 10);

        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 1706288001377);

        check_and_record_transfer<ETH>(&clock, &mut limiter, route, 9_000_000_000_000);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 90000 * 10, 0);

        // tx should fail after 10 iteration when total amount exceed 1000000 * 10^8
        let i = 0;
        while (i < 10) {
            clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
            assert!(check_and_record_transfer<ETH>(&clock, &mut limiter, route, 100_000_000_000), 0);
            i = i + 1;
        };

        // should fail on the 11th iteration
        clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
        assert!(!check_and_record_transfer<ETH>(&clock, &mut limiter, route, 100_000_000_000), 0);

        destroy(limiter);

        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }
}
