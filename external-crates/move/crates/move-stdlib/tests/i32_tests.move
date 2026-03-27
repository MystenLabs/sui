// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::i32_tests;

use std::unit_test::assert_eq;

// -- max / min --

#[test]
fun test_max() {
    assert_eq!(5i32.max(10i32), 10);
    assert_eq!((-5i32).max(5i32), 5);
    assert_eq!((-100i32).max(-50i32), -50);
    assert_eq!(2147483647i32.max(-2147483648i32), 2147483647);
}

#[test]
fun test_min() {
    assert_eq!(5i32.min(10i32), 5);
    assert_eq!((-5i32).min(5i32), -5);
    assert_eq!((-100i32).min(-50i32), -100);
    assert_eq!(2147483647i32.min(-2147483648i32), -2147483648);
}

// -- abs --

#[test]
fun test_abs() {
    assert_eq!(0i32.abs(), 0u32);
    assert_eq!(42i32.abs(), 42u32);
    assert_eq!((-42i32).abs(), 42u32);
    assert_eq!(2147483647i32.abs(), 2147483647u32);
    assert_eq!((-2147483647i32).abs(), 2147483647u32);
    assert_eq!((-2147483648i32).abs(), 2147483648u32);
}

// -- sign --

#[test]
fun test_sign() {
    assert_eq!(42i32.sign(), 1);
    assert_eq!(0i32.sign(), 0);
    assert_eq!((-42i32).sign(), -1);
    assert_eq!(2147483647i32.sign(), 1);
    assert_eq!((-2147483648i32).sign(), -1);
}

// -- pow --

#[test]
fun test_pow() {
    assert_eq!(2i32.pow(0), 1);
    assert_eq!(2i32.pow(30), 1073741824);
    assert_eq!((-2i32).pow(3), -8);
    assert_eq!((-1i32).pow(0), 1);
    assert_eq!((-1i32).pow(1), -1);
    assert_eq!((-1i32).pow(2), 1);
    assert_eq!(3i32.pow(5), 243);
    assert_eq!(0i32.pow(0), 1);
}

#[test, expected_failure(arithmetic_error, location = std::i32)]
fun test_pow_overflow() {
    2i32.pow(31);
}

// -- sqrt --

#[test]
fun test_sqrt() {
    assert_eq!(0i32.sqrt(), 0);
    assert_eq!(1i32.sqrt(), 1);
    assert_eq!(4i32.sqrt(), 2);
    assert_eq!(9i32.sqrt(), 3);
    assert_eq!(100i32.sqrt(), 10);
    assert_eq!(10000i32.sqrt(), 100);
    assert_eq!(2147483647i32.sqrt(), 46340);
    assert_eq!(15i32.sqrt(), 3);
}

#[test, expected_failure]
fun test_sqrt_negative() {
    (-1i32).sqrt();
}

// -- try_as --

#[test]
fun test_try_as_i8() {
    assert_eq!(0i32.try_as_i8(), option::some(0i8));
    assert_eq!(127i32.try_as_i8(), option::some(127i8));
    assert_eq!((-128i32).try_as_i8(), option::some(-128i8));
    assert_eq!(128i32.try_as_i8(), option::none());
    assert_eq!((-129i32).try_as_i8(), option::none());
}

#[test]
fun test_try_as_i16() {
    assert_eq!(0i32.try_as_i16(), option::some(0i16));
    assert_eq!(32767i32.try_as_i16(), option::some(32767i16));
    assert_eq!((-32768i32).try_as_i16(), option::some(-32768i16));
    assert_eq!(32768i32.try_as_i16(), option::none());
    assert_eq!((-32769i32).try_as_i16(), option::none());
}

// -- to_string --

#[test]
fun test_to_string() {
    assert_eq!(0i32.to_string(), b"0".to_string());
    assert_eq!(1i32.to_string(), b"1".to_string());
    assert_eq!((-1i32).to_string(), b"-1".to_string());
    assert_eq!(2147483647i32.to_string(), b"2147483647".to_string());
    assert_eq!((-2147483648i32).to_string(), b"-2147483648".to_string());
}

// -- do / do_eq --

#[test]
fun test_do() {
    let mut sum: i32 = 0;
    5i32.do!(|i| sum = sum + i);
    assert_eq!(sum, 10);
}

#[test]
fun test_do_eq() {
    let mut sum: i32 = 0;
    5i32.do_eq!(|i| sum = sum + i);
    assert_eq!(sum, 15);
}

#[test]
fun test_do_zero() {
    0i32.do!(|_| assert!(false));
}

// -- range_do / range_do_eq --

#[test]
fun test_range_do() {
    let mut sum: i32 = 0;
    (-3i32).range_do!(3i32, |i| sum = sum + i);
    assert_eq!(sum, -3);
}

#[test]
fun test_range_do_eq() {
    let mut sum: i32 = 0;
    (-3i32).range_do_eq!(3i32, |i| sum = sum + i);
    assert_eq!(sum, 0);
}

#[test]
fun test_range_do_empty() {
    3i32.range_do!(0i32, |_| assert!(false));
    3i32.range_do_eq!(0i32, |_| assert!(false));
}
