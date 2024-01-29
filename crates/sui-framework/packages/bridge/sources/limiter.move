// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::limiter {

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

    struct TransferLimiter has store {
        transfer_limits: VecMap<BridgeRoute, u64>,
        notional_values: VecMap<TypeName, u64>,
        // Per hour transfer amount for each bridge route
        transfer_records: VecMap<BridgeRoute, TransferRecord>,
    }

    struct TransferRecord has store {
        last_recorded_hour: u64,
        per_hour_amounts: vector<u64>,
        total_amount: u64
    }

    public fun new(transfer_limits: VecMap<BridgeRoute, u64>): TransferLimiter {
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
    ) {
        // Create record for route if not exists
        if (!vec_map::contains(&self.transfer_records, &route)) {
            vec_map::insert(&mut self.transfer_records, route, TransferRecord {
                last_recorded_hour: 0,
                per_hour_amounts: vector[],
                total_amount: 0
            })
        };
        let record = vec_map::get_mut(&mut self.transfer_records, &route);
        let current_hour_since_epoch = current_hour_since_epoch(clock);

        // First clean up old transfer histories
        let i = vector::length(&record.per_hour_amounts);
        while (i > 0 && record.last_recorded_hour < (current_hour_since_epoch - 24 + i)) {
            record.total_amount = record.total_amount - vector::remove(&mut record.per_hour_amounts, 0);
            i = i - 1;
        };
        // Backfill missing hours
        while (record.last_recorded_hour < current_hour_since_epoch) {
            // only backfill up to 24 hours
            if (record.last_recorded_hour < current_hour_since_epoch - 23) {
                record.last_recorded_hour = current_hour_since_epoch - 23;
            }else {
                record.last_recorded_hour = record.last_recorded_hour + 1;
            };
            vector::push_back(&mut record.per_hour_amounts, 0);
        };

        // Get limit for the route
        let route_limit = if (vec_map::contains(&self.transfer_limits, &route)) {
            *vec_map::get(&self.transfer_limits, &route)
        } else {
            DEFAULT_TRANSFER_LIMIT
        };

        // Compute notional amount
        let coin_type = type_name::get<T>();
        let notional_amount = *vec_map::get(&self.notional_values, &coin_type) * amount;
        let notional_amount = notional_amount / pow(10, treasury::token_decimals<T>());

        // Check if transfer amount exceed limit
        assert!(record.total_amount + notional_amount <= route_limit, ETransferAmountExceedLimit);

        // Record transfer value
        let new_amount = vector::pop_back(&mut record.per_hour_amounts) + notional_amount;
        vector::push_back(&mut record.per_hour_amounts, new_amount);
        record.total_amount = record.total_amount + notional_amount;
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
        // Notional price for ETH is 10 USD
        vec_map::insert(&mut limiter.notional_values, type_name::get<ETH>(), 10);

        let scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, 1706288001377);

        check_and_record_transfer<ETH>(&clock, &mut limiter, route, 1_000_000_000_000);

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 10000 * 10, 0);

        // transfer 1000 ETH every hour, the 24 hours totol should be 24000 * 10
        let i = 0;
        while (i < 50) {
            clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
            check_and_record_transfer<ETH>(&clock, &mut limiter, route, 100_000_000_000);
            i = i + 1;
        };

        let record = vec_map::get(&limiter.transfer_records, &route);
        assert!(record.total_amount == 24000 * 10, 0);

        destroy(limiter);

        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = ETransferAmountExceedLimit)]
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
        while (i < 11) {
            clock::increment_for_testing(&mut clock, 60 * 60 * 1000);
            check_and_record_transfer<ETH>(&clock, &mut limiter, route, 100_000_000_000);
            i = i + 1;
        };
        destroy(limiter);

        clock::destroy_for_testing(clock);
        test_scenario::end(scenario);
    }
}