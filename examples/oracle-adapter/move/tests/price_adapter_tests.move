// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module oracle_adapter::price_adapter_tests;

use oracle_adapter::price_adapter as pa;
use sui::clock;

// The reads that touch a PriceInfoObject (`price_from_pyth`, `price_or_fallback`)
// cannot be unit-tested, because a PriceInfoObject cannot be minted in a test.
// Those paths are verified by Testnet execution. These tests cover the guard
// logic and normalization on constructed prices.

fun clock_at(secs: u64): clock::Clock {
    let mut ctx = tx_context::dummy();
    let mut c = clock::create_for_testing(&mut ctx);
    clock::set_for_testing(&mut c, secs * 1000);
    c
}

#[test]
fun is_fresh_within_bound() {
    let c = clock_at(1000);
    // published 10s ago, 60s bound -> fresh
    let p = pa::new_for_testing(250, 8, true, 5, 990);
    assert!(pa::is_fresh(&p, &c, 60), 0);
    clock::destroy_for_testing(c);
}

#[test]
fun is_stale_beyond_bound() {
    let c = clock_at(1000);
    // published 100s ago, 60s bound -> stale
    let p = pa::new_for_testing(250, 8, true, 5, 900);
    assert!(!pa::is_fresh(&p, &c, 60), 0);
    clock::destroy_for_testing(c);
}

#[test]
#[expected_failure(abort_code = pa::EPriceStale)]
fun assert_fresh_aborts_on_stale() {
    let c = clock_at(1000);
    let p = pa::new_for_testing(250, 8, true, 5, 900);
    pa::assert_fresh(&p, &c, 60);
    clock::destroy_for_testing(c);
}

#[test]
fun confidence_within_bound_passes() {
    // conf 5 on price 1000 = 50 bps; bound 100 bps -> ok
    let p = pa::new_for_testing(1000, 8, true, 5, 0);
    pa::assert_confidence(&p, 100);
}

#[test]
#[expected_failure(abort_code = pa::EConfidenceTooWide)]
fun confidence_blowout_aborts() {
    // conf 20 on price 1000 = 200 bps; bound 100 bps -> abort
    let p = pa::new_for_testing(1000, 8, true, 20, 0);
    pa::assert_confidence(&p, 100);
}

#[test]
fun deviation_within_bound_passes() {
    // price 1000 vs ref 1010 = ~99 bps; bound 200 bps -> ok
    let p = pa::new_for_testing(1000, 8, true, 0, 0);
    pa::assert_within_deviation(&p, 1010, 200);
}

#[test]
#[expected_failure(abort_code = pa::EPriceDeviates)]
fun deviation_beyond_bound_aborts() {
    // price 1000 vs ref 900 = ~1111 bps; bound 100 bps -> abort
    let p = pa::new_for_testing(1000, 8, true, 0, 0);
    pa::assert_within_deviation(&p, 900, 100);
}

#[test]
fun normalize_positive_price() {
    let raw = pa::pyth_price_for_testing(250, false, 8, 5, 1000);
    let p = pa::normalize_for_testing(&raw);
    assert!(pa::magnitude(&p) == 250, 0);
    assert!(pa::confidence(&p) == 5, 1);
    assert!(pa::timestamp(&p) == 1000, 2);
    assert!(pa::expo_magnitude(&p) == 8, 3);
    assert!(pa::expo_is_negative(&p), 4);
}

#[test]
#[expected_failure(abort_code = pa::EPriceNegative)]
fun normalize_rejects_negative() {
    let raw = pa::pyth_price_for_testing(100, true, 8, 5, 1000);
    let p = pa::normalize_for_testing(&raw);
    pa::magnitude(&p);
}

#[test]
#[expected_failure(abort_code = pa::EZeroPrice)]
fun normalize_rejects_zero() {
    let raw = pa::pyth_price_for_testing(0, false, 8, 5, 1000);
    let p = pa::normalize_for_testing(&raw);
    pa::magnitude(&p);
}
