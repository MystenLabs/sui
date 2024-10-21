// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::u256_tests;

use std::{integer_tests, unit_test::assert_eq};

const BIT_SIZE: u8 = 255;
const MAX: u256 = 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF;
const MAX_PRED: u256 = MAX - 1;

const CASES: vector<u256> = vector[
    0,
    1,
    10,
    11,
    100,
    111,
    1 << (BIT_SIZE / 2),
    (1 << (BIT_SIZE / 2)) + 1,
    1 << BIT_SIZE,
    (1 << BIT_SIZE) + 1,
    MAX / 2,
    (MAX / 2) + 1,
    MAX_PRED,
    MAX,
];

#[test]
fun test_bitwise_not() {
    integer_tests::test_bitwise_not!(MAX, CASES);
}

#[test]
fun test_max() {
    integer_tests::test_max!(MAX, CASES);
}

#[test]
fun test_min() {
    integer_tests::test_min!(MAX, CASES);
}

#[test]
fun test_diff() {
    integer_tests::test_diff!(MAX, CASES);
}

#[test]
fun test_divide_and_round_up() {
    integer_tests::test_divide_and_round_up!(MAX, CASES);
}

#[test, expected_failure(arithmetic_error, location = std::u8)]
fun test_divide_and_round_up_error() {
    1u8.divide_and_round_up(0);
}

#[test]
fun test_pow() {
    integer_tests::test_pow!(MAX, CASES);
    assert_eq!(2u256.pow(12), integer_tests::slow_pow!(2u256, 12));
    assert_eq!(3u256.pow(27), integer_tests::slow_pow!(3u256, 27));
}

#[test, expected_failure(arithmetic_error, location = std::u256)]
fun test_pow_overflow() {
    255u256.pow(255);
}

#[test]
fun test_try_as_u8() {
    integer_tests::test_try_as_u8!<u256>(MAX);
}

#[test]
fun test_try_as_u16() {
    integer_tests::test_try_as_u16!<u256>(MAX);
}

#[test]
fun test_try_as_u32() {
    integer_tests::test_try_as_u32!<u256>(MAX);
}

#[test]
fun test_try_as_u64() {
    integer_tests::test_try_as_u64!<u256>(MAX);
}

#[test]
fun test_to_string() {
    integer_tests::test_to_string!<u256>();
    assert_eq!(
        (MAX / 2).to_string(),
        b"57896044618658097711785492504343953926634992332820282019728792003956564819967".to_string(),
    );
    assert_eq!(
        (MAX / 2 + 1).to_string(),
        b"57896044618658097711785492504343953926634992332820282019728792003956564819968".to_string(),
    );
    assert_eq!(
        MAX_PRED.to_string(),
        b"115792089237316195423570985008687907853269984665640564039457584007913129639934".to_string(),
    );
    assert_eq!(
        MAX.to_string(),
        b"115792089237316195423570985008687907853269984665640564039457584007913129639935".to_string(),
    );
}

#[test]
fun test_dos() {
    integer_tests::test_dos!(MAX, CASES);
}
