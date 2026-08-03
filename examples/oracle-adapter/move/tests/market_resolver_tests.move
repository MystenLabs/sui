// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module oracle_adapter::market_resolver_tests;

use oracle_adapter::market_resolver as mr;
use sui::clock;

// `resolve` touches a PriceInfoObject, which a test cannot mint, so it is
// verified by Testnet execution. These tests cover the resolvability guard: too
// early, and already settled.

fun clock_at(ms: u64): clock::Clock {
    let mut ctx = tx_context::dummy();
    let mut c = clock::create_for_testing(&mut ctx);
    clock::set_for_testing(&mut c, ms);
    c
}

#[test]
fun resolvable_after_expiry() {
    let mut ctx = tx_context::dummy();
    let m = mr::new(1_000, 60, &mut ctx);
    let c = clock_at(1_000); // exactly at expiry
    mr::assert_resolvable(&m, &c);
    clock::destroy_for_testing(c);
    mr::destroy_for_testing(m);
}

#[test]
#[expected_failure(abort_code = mr::ENotExpired)]
fun not_resolvable_before_expiry() {
    let mut ctx = tx_context::dummy();
    let m = mr::new(2_000, 60, &mut ctx);
    let c = clock_at(1_999);
    mr::assert_resolvable(&m, &c);
    clock::destroy_for_testing(c);
    mr::destroy_for_testing(m);
}

#[test]
#[expected_failure(abort_code = mr::EAlreadySettled)]
fun not_resolvable_when_settled() {
    let mut ctx = tx_context::dummy();
    let mut m = mr::new(1_000, 60, &mut ctx);
    mr::settle_for_testing(&mut m, 68_000_000);
    let c = clock_at(2_000);
    mr::assert_resolvable(&m, &c);
    clock::destroy_for_testing(c);
    mr::destroy_for_testing(m);
}
