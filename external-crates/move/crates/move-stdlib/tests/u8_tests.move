// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::u8_tests;

use std::{integer_tests, unit_test::assert_eq};

const BIT_SIZE: u8 = 8;
const MAX: u8 = 0xFF;
const MAX_PRED: u8 = MAX - 1;

const CASES: vector<u8> = vector[
    0,
    1,
    10,
    11,
    100,
    111,
    1 << (BIT_SIZE / 2 - 1),
    (1 << (BIT_SIZE / 2 - 1)) + 1,
    1 << (BIT_SIZE - 1),
    (1 << (BIT_SIZE - 1)) + 1,
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
}

#[test, expected_failure(arithmetic_error, location = std::u8)]
fun test_pow_overflow() {
    255u8.pow(255);
}

#[test]
fun test_sqrt() {
    integer_tests::test_sqrt!(MAX, CASES, vector[0, 2, 5, 8, 11, 14]);
}

#[test]
fun test_to_string() {
    integer_tests::test_to_string!<u8>();
    assert_eq!((MAX / 2).to_string(), b"127".to_string());
    assert_eq!((MAX / 2 + 1).to_string(), b"128".to_string());
    assert_eq!(MAX_PRED.to_string(), b"254".to_string());
    assert_eq!(MAX.to_string(), b"255".to_string());
}

#[test]
fun test_dos() {
    let mut sum = 0u16;
    255u8.do_eq!(|i| sum = sum + (i as u16));
    assert_eq!(sum, 32640);
    integer_tests::test_dos!(MAX, CASES);
}
