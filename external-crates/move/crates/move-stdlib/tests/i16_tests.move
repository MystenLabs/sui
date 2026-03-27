// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::i16_tests;

use std::unit_test::assert_eq;

// -- max / min --

#[test]
fun test_max() {
    assert_eq!(5i16.max(10i16), 10);
    assert_eq!((-5i16).max(5i16), 5);
    assert_eq!((-100i16).max(-50i16), -50);
    assert_eq!(32767i16.max(-32768i16), 32767);
}

#[test]
fun test_min() {
    assert_eq!(5i16.min(10i16), 5);
    assert_eq!((-5i16).min(5i16), -5);
    assert_eq!((-100i16).min(-50i16), -100);
    assert_eq!(32767i16.min(-32768i16), -32768);
}

// -- abs --

#[test]
fun test_abs() {
    assert_eq!(0i16.abs(), 0u16);
    assert_eq!(42i16.abs(), 42u16);
    assert_eq!((-42i16).abs(), 42u16);
    assert_eq!(32767i16.abs(), 32767u16);
    assert_eq!((-32767i16).abs(), 32767u16);
    assert_eq!((-32768i16).abs(), 32768u16);
}

// -- sign --

#[test]
fun test_sign() {
    assert_eq!(42i16.sign(), 1);
    assert_eq!(0i16.sign(), 0);
    assert_eq!((-42i16).sign(), -1);
    assert_eq!(32767i16.sign(), 1);
    assert_eq!((-32768i16).sign(), -1);
}

// -- pow --

#[test]
fun test_pow() {
    assert_eq!(2i16.pow(0), 1);
    assert_eq!(2i16.pow(14), 16384);
    assert_eq!((-2i16).pow(3), -8);
    assert_eq!((-1i16).pow(0), 1);
    assert_eq!((-1i16).pow(1), -1);
    assert_eq!((-1i16).pow(2), 1);
    assert_eq!(3i16.pow(5), 243);
    assert_eq!(0i16.pow(0), 1);
}

#[test, expected_failure(arithmetic_error, location = std::i16)]
fun test_pow_overflow() {
    2i16.pow(15);
}

// -- sqrt --

#[test]
fun test_sqrt() {
    assert_eq!(0i16.sqrt(), 0);
    assert_eq!(1i16.sqrt(), 1);
    assert_eq!(4i16.sqrt(), 2);
    assert_eq!(9i16.sqrt(), 3);
    assert_eq!(100i16.sqrt(), 10);
    assert_eq!(10000i16.sqrt(), 100);
    assert_eq!(32767i16.sqrt(), 181);
    assert_eq!(15i16.sqrt(), 3);
}

#[test, expected_failure]
fun test_sqrt_negative() {
    (-1i16).sqrt();
}

// -- try_as_i8 --

#[test]
fun test_try_as_i8() {
    assert_eq!(0i16.try_as_i8(), option::some(0i8));
    assert_eq!(127i16.try_as_i8(), option::some(127i8));
    assert_eq!((-128i16).try_as_i8(), option::some(-128i8));
    assert_eq!(128i16.try_as_i8(), option::none());
    assert_eq!((-129i16).try_as_i8(), option::none());
    assert_eq!(32767i16.try_as_i8(), option::none());
    assert_eq!((-32768i16).try_as_i8(), option::none());
}

// -- to_string --

#[test]
fun test_to_string() {
    assert_eq!(0i16.to_string(), b"0".to_string());
    assert_eq!(1i16.to_string(), b"1".to_string());
    assert_eq!((-1i16).to_string(), b"-1".to_string());
    assert_eq!(32767i16.to_string(), b"32767".to_string());
    assert_eq!((-32768i16).to_string(), b"-32768".to_string());
}

// -- do / do_eq --

#[test]
fun test_do() {
    let mut sum: i16 = 0;
    5i16.do!(|i| sum = sum + i);
    assert_eq!(sum, 10);
}

#[test]
fun test_do_eq() {
    let mut sum: i16 = 0;
    5i16.do_eq!(|i| sum = sum + i);
    assert_eq!(sum, 15);
}

#[test]
fun test_do_zero() {
    0i16.do!(|_| assert!(false));
}

// -- range_do / range_do_eq --

#[test]
fun test_range_do() {
    let mut sum: i16 = 0;
    (-3i16).range_do!(3i16, |i| sum = sum + i);
    assert_eq!(sum, -3);
}

#[test]
fun test_range_do_eq() {
    let mut sum: i16 = 0;
    (-3i16).range_do_eq!(3i16, |i| sum = sum + i);
    assert_eq!(sum, 0);
}

#[test]
fun test_range_do_empty() {
    3i16.range_do!(0i16, |_| assert!(false));
    3i16.range_do_eq!(0i16, |_| assert!(false));
}
