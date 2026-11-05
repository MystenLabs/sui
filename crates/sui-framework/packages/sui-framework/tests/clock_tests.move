// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::clock_tests;

use sui::clock;

#[test]
fun creating_a_clock_and_incrementing_it() {
    let mut ctx = tx_context::dummy();
    let mut clock = clock::create_for_testing(&mut ctx);

    clock.increment_for_testing(42);
    assert!(clock.timestamp_ms() == 42);

    clock.set_for_testing(50);
    assert!(clock.timestamp_ms() == 50);

    clock.destroy_for_testing();
}

#[test]
fun borrowed_clock_follows_the_test_clock() {
    assert!(clock::borrow().timestamp_ms() == 0);

    let mut ctx = tx_context::dummy();
    let mut clock = clock::create_for_testing(&mut ctx);
    clock.set_for_testing(50);
    assert!(clock::borrow().timestamp_ms() == 50);

    clock.increment_for_testing(5);
    assert!(clock::borrow().timestamp_ms() == 55);
    assert!(object::id(clock::borrow()) == object::id(&clock));

    clock.destroy_for_testing();
    assert!(clock::borrow().timestamp_ms() == 0);

    // Destroying the Clock lets the test create another, starting at 0.
    let fresh = clock::create_for_testing(&mut ctx);
    assert!(clock::borrow().timestamp_ms() == 0);
    fresh.destroy_for_testing();
}

#[test, expected_failure(abort_code = clock::EClockAlreadyExists)]
fun creating_a_second_clock_fails() {
    let mut ctx = tx_context::dummy();
    let clock = clock::create_for_testing(&mut ctx);
    let second = clock::create_for_testing(&mut ctx);
    second.destroy_for_testing();
    clock.destroy_for_testing();
}

#[test, expected_failure(abort_code = clock::EClockAlreadyExists)]
fun creating_a_second_clock_in_a_later_transaction_fails() {
    let mut test = sui::test_scenario::begin(@0x1);
    clock::create_for_testing(test.ctx()).share_for_testing();
    test.next_tx(@0x1);
    let second = clock::create_for_testing(test.ctx());
    second.destroy_for_testing();
    test.end();
}

#[test]
fun recreated_clock_can_be_shared_in_the_same_transaction() {
    let mut test = sui::test_scenario::begin(@0x1);
    clock::create_for_testing(test.ctx()).destroy_for_testing();
    clock::create_for_testing(test.ctx()).share_for_testing();
    test.next_tx(@0x1);
    let clock = test.take_shared<clock::Clock>();
    sui::test_scenario::return_shared(clock);
    test.end();
}

#[test]
fun shared_clock_can_be_destroyed_and_recreated() {
    let mut test = sui::test_scenario::begin(@0x1);
    clock::create_for_testing(test.ctx()).share_for_testing();
    test.next_tx(@0x1);
    let mut clock = test.take_shared<clock::Clock>();
    clock.increment_for_testing(7);
    clock.destroy_for_testing();
    clock::create_for_testing(test.ctx()).share_for_testing();
    test.next_tx(@0x1);
    let clock = test.take_shared<clock::Clock>();
    assert!(clock.timestamp_ms() == 0);
    assert!(clock::borrow().timestamp_ms() == 0);
    sui::test_scenario::return_shared(clock);
    test.end();
}
