// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::limiter {

    use std::option;
    use std::type_name;
    use std::type_name::TypeName;
    use std::vector;
    use sui::clock;
    use sui::clock::Clock;
    use sui::math::pow;
    use sui::vec_map;
    use sui::vec_map::VecMap;
    use bridge::treasury;
    use bridge::chain_ids::BridgeRoute;
    #[test_only]
    use std::debug::print;
    #[test_only]
    use sui::test_scenario;
    #[test_only]
    use sui::test_utils::destroy;
    #[test_only]
    use bridge::chain_ids;
    #[test_only]
    use bridge::eth::ETH;

    const ETransferAmountExceedLimit: u64 = 0;

    // TODO: Make this configurable?
    const DEFAULT_TRANSFER_LIMIT: u64 = 0;

    const TRANSFER_AMOUNT_DP: u8 = 4;

    struct TransferLimiter has store {
        transfer_limits: VecMap<BridgeRoute, u64>,
        // USD notional value, 4 DP accuracy
        notional_values: VecMap<TypeName, u64>,
        // Per hour transfer amount for each bridge route
        transfer_records: VecMap<BridgeRoute, TransferRecord>,
    }

    struct TransferRecord has store {
        hour_head: u64,
        hour_tail: u64,
        per_hour_amounts: vector<u64>,
        // total amount in USD, 4 DP accuracy
        total_amount: u64
    }

    public fun new(): TransferLimiter {
        TransferLimiter {
            transfer_limits: vec_map::empty(),
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
                hour_head: 0,
                hour_tail: 0,
                per_hour_amounts: vector[],
                total_amount: 0
            })
        };
        let record = vec_map::get_mut(&mut self.transfer_records, &route);
        let current_hour_since_epoch = current_hour_since_epoch(clock);

        // First clean up old transfer histories
        remove_stale_hour_maybe(record, current_hour_since_epoch);
        // Backfill missing hours
        append_new_empty_hours(record, current_hour_since_epoch);

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
        let new_amount = vector::pop_back(&mut record.per_hour_amounts) + notional_amount;
        vector::push_back(&mut record.per_hour_amounts, new_amount);
        record.total_amount = record.total_amount + notional_amount;
        return true
    }

    fun append_new_empty_hours(self: &mut TransferRecord, current_hour_since_epoch: u64) {
        if (self.hour_head == current_hour_since_epoch) {
            return; // nothing to backfill
        };

        // If tail is even older than 24 hours ago, advance it to that.
        let target_tail = current_hour_since_epoch - 24;
        if (self.hour_tail < target_tail) {
            self.hour_tail = target_tail;
        };

        // If old head is even older than target tail, advance it to that.
        if (self.hour_head < target_tail) {
            self.hour_head = target_tail;
        };

        // Backfill from head to current hour
        while (self.hour_head < current_hour_since_epoch) {
            self.hour_head = self.hour_head + 1;
            vector::push_back(&mut self.per_hour_amounts, 0);
        }
    }

    fun remove_stale_hour_maybe(self: &mut TransferRecord, current_hour_since_epoch: u64) {
        // remove tails until it's within 24 hours range
        while (self.hour_tail + 24 < current_hour_since_epoch && self.hour_tail < self.hour_head) {
            self.total_amount = self.total_amount - vector::remove(&mut self.per_hour_amounts, 0);
            self.hour_tail = self.hour_tail + 1;
        }
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

        // Global transfer limit is 1M USD
        vec_map::insert(&mut limiter.transfer_limits, route, 1_000_000);
        // Notional price for ETH is 5 USD
        vec_map::insert(&mut limiter.notional_values, type_name::get<ETH>(), 5);

        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 1706288001377);

        check_and_record_transfer<ETH>(&clock, &mut limiter, route, 1_000_000_000_000);

        let record = vec_map::get(&limiter.transfer_records, &route);
        print(&record.total_amount);
        assert!(record.total_amount == 10000 * 5, 0);

        // transfer 1000 ETH every hour, the 24 hours totol should be 24000 * 10
        let i = 0;
        while (i < 50) {
            clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
            check_and_record_transfer<ETH>(&clock, &mut limiter, route, 100_000_000_000);
            i = i + 1;
        };

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 24000 * 5, 0);

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
