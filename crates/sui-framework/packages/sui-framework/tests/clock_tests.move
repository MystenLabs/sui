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

    // A fresh test Clock starts at 0 and takes over.
    let fresh = clock::create_for_testing(&mut ctx);
    assert!(clock::borrow().timestamp_ms() == 0);

    fresh.destroy_for_testing();
    clock.destroy_for_testing();
}
