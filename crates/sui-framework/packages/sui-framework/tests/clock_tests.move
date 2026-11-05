// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::clock_tests;

use std::unit_test::assert_eq;
use sui::clock;
use sui::test_scenario;

#[test, allow(deprecated_usage)]
fun creating_a_clock_and_incrementing_it() {
    let mut ctx = tx_context::dummy();
    let mut clock = clock::create_for_testing(&mut ctx);

    clock.increment_for_testing(42);
    assert!(clock.timestamp_ms() == 42);

    clock.set_for_testing(50);
    assert!(clock.timestamp_ms() == 50);

    clock.destroy_for_testing();
}

#[test, allow(deprecated_usage)]
fun borrowed_clock_follows_the_test_clock() {
    assert!(clock::borrow().timestamp_ms() == 0);

    let mut ctx = tx_context::dummy();
    let mut clock = clock::create_for_testing(&mut ctx);
    clock.set_for_testing(50);
    assert!(clock::borrow().timestamp_ms() == 50);

    clock.increment_for_testing(5);
    assert!(clock::borrow().timestamp_ms() == 55);

    // A fresh test Clock starts at the current time.
    let fresh = clock::create_for_testing(&mut ctx);
    assert!(fresh.timestamp_ms() == 55);
    assert!(clock::borrow().timestamp_ms() == 55);

    fresh.destroy_for_testing();
    clock.destroy_for_testing();
}

#[test, allow(deprecated_usage)]
fun deprecated_setters_update_outstanding_borrows() {
    let borrowed = clock::borrow();
    let mut ctx = tx_context::dummy();
    let mut clock = clock::create_for_testing(&mut ctx);

    clock.set_for_testing(50);
    assert_eq!(borrowed.timestamp_ms(), 50);

    clock.increment_for_testing(5);
    assert_eq!(borrowed.timestamp_ms(), 55);
    assert_eq!(clock::borrow().timestamp_ms(), 55);

    clock.destroy_for_testing();
}

#[test, allow(deprecated_usage)]
fun deprecated_increment_reads_the_current_time() {
    let mut ctx = tx_context::dummy();
    let mut stale = clock::create_for_testing(&mut ctx);
    let mut fresh = clock::create_for_testing(&mut ctx);

    fresh.increment_for_testing(10);
    stale.increment_for_testing(5);
    assert_eq!(stale.timestamp_ms(), 15);
    assert_eq!(clock::borrow().timestamp_ms(), 15);

    stale.destroy_for_testing();
    fresh.destroy_for_testing();
}

#[test, allow(deprecated_usage), expected_failure(abort_code = clock::ETimestampDecreased)]
fun deprecated_set_rejects_an_earlier_time() {
    let mut ctx = tx_context::dummy();
    let mut stale = clock::create_for_testing(&mut ctx);
    let mut fresh = clock::create_for_testing(&mut ctx);

    fresh.set_for_testing(100);
    stale.set_for_testing(50);
    abort
}

#[test, allow(deprecated_usage)]
fun create_for_testing_keeps_the_scenario_time() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_clock_timestamp(100);
    scenario.next_with_context(builder);

    let clock = clock::create_for_testing(scenario.ctx());
    assert_eq!(clock.timestamp_ms(), 100);
    assert_eq!(clock::borrow().timestamp_ms(), 100);
    clock.destroy_for_testing();

    // Each of the two transactions `create_system_objects` ends advances the Clock.
    scenario.create_system_objects();
    assert_eq!(clock::borrow().timestamp_ms(), 102);
    let clock = scenario.take_shared<clock::Clock>();
    assert_eq!(clock.timestamp_ms(), 101);
    test_scenario::return_shared(clock);
    scenario.end();
}

#[test]
fun scenario_clock_starts_at_zero() {
    let scenario = test_scenario::begin(@0xA);
    assert_eq!(clock::borrow().timestamp_ms(), 0);
    scenario.end();
}

#[test]
fun new_transactions_advance_the_clock() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_clock_timestamp(100);
    scenario.next_with_context(builder);
    scenario.next_tx(@0xB);
    assert_eq!(clock::borrow().timestamp_ms(), 101);
    scenario.next_epoch(@0xB);
    assert_eq!(clock::borrow().timestamp_ms(), 102);
    scenario.later_epoch(1000, @0xB);
    assert_eq!(clock::borrow().timestamp_ms(), 103);
    scenario.skip_to_epoch(5);
    assert_eq!(clock::borrow().timestamp_ms(), 104);
    scenario.skip_to_epoch(5);
    assert_eq!(clock::borrow().timestamp_ms(), 104);
    scenario.end();
}

#[test]
fun begin_with_context_keeps_the_clock_by_default() {
    let scenario = test_scenario::begin_with_context(test_scenario::ctx_builder_from_sender(@0xA));
    assert_eq!(clock::borrow().timestamp_ms(), 0);
    scenario.end();
}

#[test]
fun begin_with_context_sets_the_clock() {
    let builder = test_scenario::ctx_builder_from_sender(@0xA).set_clock_timestamp(100);
    let scenario = test_scenario::begin_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 100);
    scenario.end();
}

#[test]
fun next_with_context_sets_the_clock() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_clock_timestamp(100);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 100);

    let builder = scenario.ctx_builder().set_clock_timestamp(250);

    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 250);
    scenario.end();
}

#[test]
fun next_with_context_increments_the_clock() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().increment_clock_timestamp(10);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 10);

    let builder = scenario.ctx_builder().increment_clock_timestamp(5).increment_clock_timestamp(7);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 22);

    let builder = scenario.ctx_builder().set_clock_timestamp(100).increment_clock_timestamp(1);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 101);
    scenario.end();
}

#[test]
fun clock_setters_combine_on_one_builder() {
    let mut scenario = test_scenario::begin(@0xA);
    // Each set is checked against the Clock, not the pending time, so a later set can be lower.
    let builder = scenario.ctx_builder().set_clock_timestamp(200).set_clock_timestamp(100);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 100);

    let builder = scenario.ctx_builder().increment_clock_timestamp(5).increment_clock_timestamp(7);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 112);

    let builder = scenario.ctx_builder().increment_clock_timestamp(1000).set_clock_timestamp(150);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 150);

    let builder = scenario.ctx_builder().set_clock_timestamp(200).increment_clock_timestamp(10);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 210);

    let builder = scenario
        .ctx_builder()
        .increment_clock_timestamp(10)
        .set_clock_timestamp(300)
        .increment_clock_timestamp(5)
        .set_clock_timestamp(400)
        .increment_clock_timestamp(1);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 401);
    scenario.end();
}

#[test, expected_failure(abort_code = test_scenario::EClockTimestampNotIncreased)]
fun second_set_clock_timestamp_rejects_the_current_timestamp() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_clock_timestamp(100);
    scenario.next_with_context(builder);
    let _builder = scenario.ctx_builder().set_clock_timestamp(200).set_clock_timestamp(100);
    abort
}

#[test]
fun next_with_context_advances_the_clock_by_default() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_clock_timestamp(100);
    scenario.next_with_context(builder);
    let builder = scenario.ctx_builder();
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 101);
    let rgp = scenario.ctx().reference_gas_price();
    let builder = test_scenario::ctx_builder_from_sender(@0xB).set_reference_gas_price(rgp);
    scenario.next_with_context(builder);
    assert_eq!(clock::borrow().timestamp_ms(), 102);
    scenario.end();
}

#[test]
fun next_with_context_advances_the_clock_once_across_epochs() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_epoch(3);
    scenario.next_with_context(builder);
    assert_eq!(scenario.ctx().epoch(), 3);
    assert_eq!(clock::borrow().timestamp_ms(), 1);
    scenario.end();
}

#[test, expected_failure(abort_code = test_scenario::EClockTimestampNotIncreased)]
fun set_clock_timestamp_rejects_the_current_timestamp() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_clock_timestamp(100);
    scenario.next_with_context(builder);
    let _builder = scenario.ctx_builder().set_clock_timestamp(100);
    abort
}

#[test, expected_failure(abort_code = test_scenario::EClockTimestampNotIncreased)]
fun increment_clock_timestamp_rejects_zero() {
    let scenario = test_scenario::begin(@0xA);
    let _builder = scenario.ctx_builder().increment_clock_timestamp(0);
    abort
}

#[test]
fun next_with_context_sets_the_clock_across_epochs() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_epoch(3).set_clock_timestamp(100);
    scenario.next_with_context(builder);
    assert_eq!(scenario.ctx().epoch(), 3);
    assert_eq!(clock::borrow().timestamp_ms(), 100);
    scenario.end();
}

#[test]
fun next_with_context_updates_outstanding_borrows() {
    let mut scenario = test_scenario::begin(@0xA);
    let borrowed = clock::borrow();
    assert_eq!(borrowed.timestamp_ms(), 0);

    let builder = scenario.ctx_builder().set_clock_timestamp(100);

    scenario.next_with_context(builder);
    assert_eq!(borrowed.timestamp_ms(), 100);

    let builder = scenario.ctx_builder().increment_clock_timestamp(5);

    scenario.next_with_context(builder);
    assert_eq!(borrowed.timestamp_ms(), 105);
    scenario.end();
}

#[test, expected_failure(abort_code = test_scenario::EClockTimestampNotIncreased)]
fun next_with_context_rejects_an_earlier_timestamp() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_clock_timestamp(100);
    scenario.next_with_context(builder);
    let builder = scenario.ctx_builder().set_clock_timestamp(99);
    scenario.next_with_context(builder);
    abort
}

#[test, expected_failure(abort_code = test_scenario::EClockTimestampNotIncreased)]
fun next_with_context_rejects_a_stale_builder() {
    let mut scenario = test_scenario::begin(@0xA);
    let stale = scenario.ctx_builder().increment_clock_timestamp(10);
    let builder = scenario.ctx_builder().set_clock_timestamp(100);
    scenario.next_with_context(builder);
    scenario.next_with_context(stale);
    abort
}

#[test, expected_failure(abort_code = test_scenario::EClockTimestampNotIncreased)]
fun next_with_context_rejects_an_earlier_timestamp_across_epochs() {
    let mut scenario = test_scenario::begin(@0xA);
    let builder = scenario.ctx_builder().set_clock_timestamp(100);
    scenario.next_with_context(builder);
    let builder = scenario.ctx_builder().set_epoch(2).set_clock_timestamp(99);
    scenario.next_with_context(builder);
    abort
}

#[
    test,
    allow(
        deprecated_usage,
    ),
    expected_failure(
        abort_code = test_scenario::EClockTimestampNotIncreased,
    ),
]
fun begin_with_context_rejects_an_earlier_timestamp() {
    let mut ctx = tx_context::dummy();
    let mut clock = clock::create_for_testing(&mut ctx);
    clock.set_for_testing(100);
    clock.destroy_for_testing();
    let builder = test_scenario::ctx_builder_from_sender(@0xA).set_clock_timestamp(99);
    let _scenario = test_scenario::begin_with_context(builder);
    abort
}
