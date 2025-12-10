// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::funds_accumulator_tests;

use std::unit_test::assert_eq;
use sui::funds_accumulator::create_withdrawal;

public struct TestToken has store {}

#[random_test]
fun test_withdrawal_fields(owner: address, limit: u256) {
    let withdrawal = create_withdrawal<TestToken>(owner, limit);
    assert_eq!(withdrawal.owner(), owner);
    assert_eq!(withdrawal.limit(), limit);
}

#[random_test]
fun test_withdrawal_split(owner: address) {
    let initial = 1000;
    let mut withdrawal = create_withdrawal<TestToken>(owner, initial);
    let split = 300;
    let sub = withdrawal.split(split);

    assert_eq!(withdrawal.limit(), initial - split);
    assert_eq!(sub.owner(), owner);
    assert_eq!(sub.limit(), split);
}

#[random_test]
fun test_withdrawal_split_zero(owner: address, limit: u256) {
    let mut withdrawal = create_withdrawal<TestToken>(owner, limit);
    let sub = withdrawal.split(0);

    assert_eq!(withdrawal.limit(), limit);
    assert_eq!(sub.owner(), owner);
    assert_eq!(sub.limit(), 0);
}

#[random_test]
fun test_withdrawal_split_full(owner: address, limit: u256) {
    let mut withdrawal = create_withdrawal<TestToken>(owner, limit);
    let sub = withdrawal.split(limit);

    assert_eq!(withdrawal.limit(), 0);
    assert_eq!(sub.owner(), owner);
    assert_eq!(sub.limit(), limit);
}

#[random_test]
#[expected_failure(abort_code = sui::funds_accumulator::EInvalidSubLimit)]
fun test_withdrawal_split_exceeds_limit(owner: address, limit: u128) {
    let limit = limit as u256;
    let mut withdrawal = create_withdrawal<TestToken>(owner, limit);
    let _sub = withdrawal.split(limit + 1);
}

#[random_test]
fun test_withdrawal_join(owner: address, limit1: u128, limit2: u128) {
    let limit1 = limit1 as u256;
    let limit2 = limit2 as u256;
    let mut withdrawal1 = create_withdrawal<TestToken>(owner, limit1);
    let withdrawal2 = create_withdrawal<TestToken>(owner, limit2);
    withdrawal1.join(withdrawal2);
    assert_eq!(withdrawal1.owner(), owner);
    assert_eq!(withdrawal1.limit(), limit1 + limit2);
}

#[random_test]
fun test_withdrawal_join_zero(owner: address, limit: u256) {
    // non-zero joined with zero
    let mut non_zero = create_withdrawal<TestToken>(owner, limit);
    let zero = create_withdrawal<TestToken>(owner, 0);
    non_zero.join(zero);
    assert_eq!(non_zero.owner(), owner);
    assert_eq!(non_zero.limit(), limit);

    // zero joined with non-zero
    let mut zero = create_withdrawal<TestToken>(owner, 0);
    let non_zero = create_withdrawal<TestToken>(owner, limit);
    zero.join(non_zero);
    assert_eq!(zero.owner(), owner);
    assert_eq!(zero.limit(), limit);
}

#[test]
#[expected_failure(abort_code = sui::funds_accumulator::EOwnerMismatch)]
fun test_withdrawal_join_different_owners() {
    let owner1 = @0x1;
    let owner2 = @0x2;
    let mut withdrawal1 = create_withdrawal<TestToken>(owner1, 500);
    let withdrawal2 = create_withdrawal<TestToken>(owner2, 300);
    withdrawal1.join(withdrawal2);
}

#[random_test]
#[expected_failure(abort_code = sui::funds_accumulator::EOverflow)]
fun test_withdrawal_join_overflow(owner: address) {
    let max_value = std::u256::max_value!();
    let mut withdrawal1 = create_withdrawal<TestToken>(owner, max_value);
    let withdrawal2 = create_withdrawal<TestToken>(owner, 1);
    withdrawal1.join(withdrawal2);
}

#[random_test]
fun test_withdrawal_split_join(owner: address, l1: u128, l2: u128, l3: u128) {
    let l1 = l1 as u256;
    let l2 = l2 as u256;
    let l3 = l3 as u256;
    // ensure l1 > l2 + l3
    let mut w1 = create_withdrawal<TestToken>(owner, l1 + l2 + l3);
    let w2 = w1.split(l2);
    let w3 = w1.split(l3);

    assert_eq!(w1.limit(), l1);
    assert_eq!(w2.limit(), l2);
    assert_eq!(w3.limit(), l3);

    w1.join(w2);
    assert_eq!(w1.limit(), l1 + l2);

    w1.join(w3);
    assert_eq!(w1.limit(), l1 + l2 + l3);
}

public struct TestObject has key {
    id: UID,
}
