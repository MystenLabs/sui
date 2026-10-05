// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::funds_accumulator_tests;

use std::unit_test::assert_eq;
use sui::balance::{Self, Balance};
use sui::funds_accumulator::create_withdrawal;
use sui::sui::SUI;
use sui::test_scenario;

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

fun withdraw_from_object<T>(obj: &mut TestObject, value: u64): u64 {
    balance::redeem_funds(
        balance::withdraw_funds_from_object<T>(&mut obj.id, value),
    ).destroy_for_testing()
}

fun withdraw_from_address<T>(owner: address, value: u64): u64 {
    balance::redeem_funds(
        test_scenario::withdraw_balance_from_address<T>(owner, value),
    ).destroy_for_testing()
}

#[test]
fun test_object_funds_withdraw() {
    let mut scenario = test_scenario::begin(@0x0);
    let mut obj = TestObject { id: scenario.new_object() };
    let owner = obj.id.to_address();
    balance::create_for_testing<TestToken>(1000).send_funds(owner);
    assert_eq!(test_scenario::settled_balance<TestToken>(owner), 0);

    scenario.next_tx(@0x0);
    assert_eq!(test_scenario::settled_balance<TestToken>(owner), 1000);
    assert_eq!(withdraw_from_object<TestToken>(&mut obj, 400), 400);

    scenario.next_tx(@0x0);
    assert_eq!(test_scenario::settled_balance<TestToken>(owner), 600);
    assert_eq!(withdraw_from_object<TestToken>(&mut obj, 600), 600);

    scenario.next_tx(@0x0);
    assert_eq!(test_scenario::settled_balance<TestToken>(owner), 0);

    let TestObject { id } = obj;
    id.delete();
    scenario.end();
}

#[test]
fun test_object_funds_withdraw_uses_deposit_in_same_tx() {
    let mut scenario = test_scenario::begin(@0x0);
    let mut obj = TestObject { id: scenario.new_object() };
    let owner = obj.id.to_address();
    balance::create_for_testing<TestToken>(1000).send_funds(owner);

    scenario.next_tx(@0x0);
    balance::create_for_testing<TestToken>(500).send_funds(owner);
    assert_eq!(withdraw_from_object<TestToken>(&mut obj, 1500), 1500);

    let TestObject { id } = obj;
    id.delete();
    scenario.end();
}

#[test]
fun test_object_funds_withdraw_zero_without_deposit() {
    let mut scenario = test_scenario::begin(@0x0);
    let mut obj = TestObject { id: scenario.new_object() };
    assert_eq!(withdraw_from_object<TestToken>(&mut obj, 0), 0);

    let TestObject { id } = obj;
    id.delete();
    scenario.end();
}

#[test]
#[expected_failure(abort_code = 5, location = sui::funds_accumulator)]
fun test_object_funds_withdraw_insufficient() {
    let mut scenario = test_scenario::begin(@0x0);
    let mut obj = TestObject { id: scenario.new_object() };
    balance::create_for_testing<TestToken>(1000).send_funds(obj.id.to_address());

    scenario.next_tx(@0x0);
    withdraw_from_object<TestToken>(&mut obj, 1001);
    abort
}

#[test]
#[expected_failure(abort_code = 5, location = sui::funds_accumulator)]
fun test_object_funds_withdraw_after_earlier_withdraw() {
    let mut scenario = test_scenario::begin(@0x0);
    let mut obj = TestObject { id: scenario.new_object() };
    balance::create_for_testing<TestToken>(1000).send_funds(obj.id.to_address());

    scenario.next_tx(@0x0);
    withdraw_from_object<TestToken>(&mut obj, 400);

    scenario.next_tx(@0x0);
    withdraw_from_object<TestToken>(&mut obj, 601);
    abort
}

#[test]
fun test_address_funds_withdraw() {
    let mut scenario = test_scenario::begin(@0xA);
    balance::create_for_testing<TestToken>(1000).send_funds(@0xA);
    balance::create_for_testing<TestToken>(500).send_funds(@0xB);
    balance::create_for_testing<SUI>(700).send_funds(@0xA);
    assert_eq!(test_scenario::settled_balance<TestToken>(@0xA), 0);

    scenario.next_tx(@0xA);
    assert_eq!(test_scenario::settled_balance<TestToken>(@0xA), 1000);
    assert_eq!(withdraw_from_address<TestToken>(@0xA, 300), 300);
    assert_eq!(withdraw_from_address<TestToken>(@0xA, 100), 100);
    assert_eq!(withdraw_from_address<TestToken>(@0xB, 500), 500);
    assert_eq!(withdraw_from_address<SUI>(@0xA, 700), 700);

    scenario.next_tx(@0xA);
    assert_eq!(test_scenario::settled_balance<TestToken>(@0xA), 600);
    assert_eq!(test_scenario::settled_balance<TestToken>(@0xB), 0);
    assert_eq!(test_scenario::settled_balance<SUI>(@0xA), 0);
    assert_eq!(withdraw_from_address<TestToken>(@0xA, 600), 600);

    scenario.next_tx(@0xA);
    assert_eq!(test_scenario::settled_balance<TestToken>(@0xA), 0);
    scenario.end();
}

#[test]
fun test_unreserved_withdrawal_nets_with_deposits() {
    let mut scenario = test_scenario::begin(@0xA);
    balance::redeem_funds(create_withdrawal<Balance<TestToken>>(@0xA, 100)).send_funds(@0xA);

    scenario.next_tx(@0xA);
    assert_eq!(test_scenario::settled_balance<TestToken>(@0xA), 0);
    scenario.end();
}

#[test]
#[expected_failure(abort_code = test_scenario::EZeroWithdrawal)]
fun test_address_funds_withdraw_zero_without_deposit() {
    let _scenario = test_scenario::begin(@0xA);
    withdraw_from_address<TestToken>(@0xA, 0);
    abort
}

#[test]
#[expected_failure(abort_code = test_scenario::EInsufficientFunds)]
fun test_address_funds_withdraw_without_deposit() {
    let _scenario = test_scenario::begin(@0xA);
    withdraw_from_address<TestToken>(@0xA, 1);
    abort
}

#[test]
#[expected_failure(abort_code = test_scenario::EBalanceOverflow)]
fun test_settled_balance_overflow() {
    let mut scenario = test_scenario::begin(@0xA);
    balance::create_for_testing<TestToken>(std::u64::max_value!()).send_funds(@0xA);

    scenario.next_tx(@0xA);
    balance::create_for_testing<TestToken>(1).send_funds(@0xA);

    scenario.next_tx(@0xA);
    test_scenario::settled_balance<TestToken>(@0xA);
    abort
}

#[test]
#[expected_failure(abort_code = test_scenario::EUnbackedWithdrawal)]
fun test_withdrawal_kept_across_transactions_is_unbacked() {
    let mut scenario = test_scenario::begin(@0xA);
    balance::create_for_testing<TestToken>(1000).send_funds(@0xA);

    scenario.next_tx(@0xA);
    let kept = test_scenario::withdraw_balance_from_address<TestToken>(@0xA, 1000);

    scenario.next_tx(@0xA);
    let reserved = test_scenario::withdraw_balance_from_address<TestToken>(@0xA, 1000);
    balance::redeem_funds(kept).destroy_for_testing();
    balance::redeem_funds(reserved).destroy_for_testing();
    scenario.next_tx(@0xA);
    abort
}

#[test]
#[expected_failure(abort_code = test_scenario::EInsufficientFunds)]
fun test_address_funds_withdraw_insufficient() {
    let mut scenario = test_scenario::begin(@0xA);
    balance::create_for_testing<TestToken>(1000).send_funds(@0xA);

    scenario.next_tx(@0xA);
    withdraw_from_address<TestToken>(@0xA, 600);
    withdraw_from_address<TestToken>(@0xA, 401);
    abort
}

#[test]
#[expected_failure(abort_code = test_scenario::EInsufficientFunds)]
fun test_address_funds_withdraw_reserved_without_redeem() {
    let mut scenario = test_scenario::begin(@0xA);
    balance::create_for_testing<TestToken>(1000).send_funds(@0xA);

    scenario.next_tx(@0xA);
    let _reserved = test_scenario::withdraw_balance_from_address<TestToken>(@0xA, 600);
    withdraw_from_address<TestToken>(@0xA, 401);
    abort
}

#[test]
#[expected_failure(abort_code = test_scenario::EInsufficientFunds)]
fun test_address_funds_withdraw_ignores_deposit_in_same_tx() {
    let mut scenario = test_scenario::begin(@0xA);
    balance::create_for_testing<TestToken>(1000).send_funds(@0xA);

    scenario.next_tx(@0xA);
    balance::create_for_testing<TestToken>(500).send_funds(@0xA);
    withdraw_from_address<TestToken>(@0xA, 1001);
    abort
}
