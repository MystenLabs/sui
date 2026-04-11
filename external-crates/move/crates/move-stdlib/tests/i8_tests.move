// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::i8_tests;

use std::unit_test::assert_eq;

// -- max / min --

#[test]
fun test_max() {
    assert_eq!(5i8.max(10i8), 10);
    assert_eq!(10i8.max(5i8), 10);
    assert_eq!((-5i8).max(5i8), 5);
    assert_eq!((-10i8).max(-5i8), -5);
    assert_eq!(0i8.max(0i8), 0);
    assert_eq!(127i8.max(-128i8), 127);
    assert_eq!((-128i8).max(127i8), 127);
}

#[test]
fun test_min() {
    assert_eq!(5i8.min(10i8), 5);
    assert_eq!(10i8.min(5i8), 5);
    assert_eq!((-5i8).min(5i8), -5);
    assert_eq!((-10i8).min(-5i8), -10);
    assert_eq!(0i8.min(0i8), 0);
    assert_eq!(127i8.min(-128i8), -128);
    assert_eq!((-128i8).min(127i8), -128);
}

// -- abs --

#[test]
fun test_abs() {
    assert_eq!(0i8.abs(), 0u8);
    assert_eq!(42i8.abs(), 42u8);
    assert_eq!((-42i8).abs(), 42u8);
    assert_eq!(127i8.abs(), 127u8);
    assert_eq!((-127i8).abs(), 127u8);
    assert_eq!((-128i8).abs(), 128u8);
}

// -- sign --

#[test]
fun test_sign() {
    assert_eq!(42i8.sign(), 1);
    assert_eq!(0i8.sign(), 0);
    assert_eq!((-42i8).sign(), -1);
    assert_eq!(127i8.sign(), 1);
    assert_eq!((-128i8).sign(), -1);
}

// -- pow --

#[test]
fun test_pow() {
    assert_eq!(2i8.pow(0), 1);
    assert_eq!(2i8.pow(1), 2);
    assert_eq!(2i8.pow(6), 64);
    assert_eq!((-2i8).pow(3), -8);
    assert_eq!((-1i8).pow(0), 1);
    assert_eq!((-1i8).pow(1), -1);
    assert_eq!((-1i8).pow(2), 1);
    assert_eq!(0i8.pow(0), 1);
    assert_eq!(0i8.pow(5), 0);
    assert_eq!(1i8.pow(127), 1);
    assert_eq!(3i8.pow(4), 81);
}

#[test, expected_failure(arithmetic_error, location = std::i8)]
fun test_pow_overflow() {
    2i8.pow(7);
}

// -- sqrt --

#[test]
fun test_sqrt() {
    assert_eq!(0i8.sqrt(), 0);
    assert_eq!(1i8.sqrt(), 1);
    assert_eq!(4i8.sqrt(), 2);
    assert_eq!(9i8.sqrt(), 3);
    assert_eq!(100i8.sqrt(), 10);
    assert_eq!(127i8.sqrt(), 11);
    assert_eq!(8i8.sqrt(), 2);
    assert_eq!(15i8.sqrt(), 3);
}

#[test, expected_failure]
fun test_sqrt_negative() {
    (-1i8).sqrt();
}

// -- to_string --

#[test]
fun test_to_string() {
    assert_eq!(0i8.to_string(), b"0".to_string());
    assert_eq!(1i8.to_string(), b"1".to_string());
    assert_eq!((-1i8).to_string(), b"-1".to_string());
    assert_eq!(127i8.to_string(), b"127".to_string());
    assert_eq!((-128i8).to_string(), b"-128".to_string());
    assert_eq!(42i8.to_string(), b"42".to_string());
    assert_eq!((-42i8).to_string(), b"-42".to_string());
}

// -- do / do_eq --

#[test]
fun test_do() {
    let mut sum: i8 = 0;
    5i8.do!(|i| sum = sum + i);
    // sum = 0 + 1 + 2 + 3 + 4 = 10
    assert_eq!(sum, 10);
}

#[test]
fun test_do_eq() {
    let mut sum: i8 = 0;
    5i8.do_eq!(|i| sum = sum + i);
    // sum = 0 + 1 + 2 + 3 + 4 + 5 = 15
    assert_eq!(sum, 15);
}

#[test]
fun test_do_zero() {
    0i8.do!(|_| assert!(false));
}

// -- range_do / range_do_eq --

#[test]
fun test_range_do() {
    let mut sum: i8 = 0;
    (-3i8).range_do!(3i8, |i| sum = sum + i);
    // sum = -3 + -2 + -1 + 0 + 1 + 2 = -3
    assert_eq!(sum, -3);
}

#[test]
fun test_range_do_eq() {
    let mut sum: i8 = 0;
    (-3i8).range_do_eq!(3i8, |i| sum = sum + i);
    // sum = -3 + -2 + -1 + 0 + 1 + 2 + 3 = 0
    assert_eq!(sum, 0);
}

#[test]
fun test_range_do_empty() {
    3i8.range_do!(0i8, |_| assert!(false));
    3i8.range_do_eq!(0i8, |_| assert!(false));
}
