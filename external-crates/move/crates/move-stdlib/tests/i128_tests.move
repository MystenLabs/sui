// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::i128_tests;

use std::unit_test::assert_eq;

const MAX: i128 = 170141183460469231731687303715884105727;
const MIN: i128 = -170141183460469231731687303715884105728;

// -- max / min --

#[test]
fun test_max() {
    assert_eq!(5i128.max(10i128), 10);
    assert_eq!((-5i128).max(5i128), 5);
    assert_eq!((-100i128).max(-50i128), -50);
    assert_eq!(MAX.max(MIN), MAX);
}

#[test]
fun test_min() {
    assert_eq!(5i128.min(10i128), 5);
    assert_eq!((-5i128).min(5i128), -5);
    assert_eq!((-100i128).min(-50i128), -100);
    assert_eq!(MAX.min(MIN), MIN);
}

// -- abs --

#[test]
fun test_abs() {
    assert_eq!(0i128.abs(), 0u128);
    assert_eq!(42i128.abs(), 42u128);
    assert_eq!((-42i128).abs(), 42u128);
    assert_eq!(MAX.abs(), 170141183460469231731687303715884105727u128);
    assert_eq!(MIN.abs(), 170141183460469231731687303715884105728u128);
}

// -- sign --

#[test]
fun test_sign() {
    assert_eq!(42i128.sign(), 1);
    assert_eq!(0i128.sign(), 0);
    assert_eq!((-42i128).sign(), -1);
    assert_eq!(MAX.sign(), 1);
    assert_eq!(MIN.sign(), -1);
}

// -- pow --

#[test]
fun test_pow() {
    assert_eq!(2i128.pow(0), 1);
    assert_eq!(2i128.pow(10), 1024);
    assert_eq!((-2i128).pow(3), -8);
    assert_eq!((-1i128).pow(0), 1);
    assert_eq!((-1i128).pow(1), -1);
    assert_eq!((-1i128).pow(2), 1);
    assert_eq!(3i128.pow(5), 243);
    assert_eq!(0i128.pow(0), 1);
}

#[test, expected_failure(arithmetic_error, location = std::i128)]
fun test_pow_overflow() {
    2i128.pow(127);
}

// -- sqrt --

#[test]
fun test_sqrt() {
    assert_eq!(0i128.sqrt(), 0);
    assert_eq!(1i128.sqrt(), 1);
    assert_eq!(4i128.sqrt(), 2);
    assert_eq!(9i128.sqrt(), 3);
    assert_eq!(100i128.sqrt(), 10);
    assert_eq!(10000i128.sqrt(), 100);
    assert_eq!(15i128.sqrt(), 3);
}

#[test, expected_failure]
fun test_sqrt_negative() {
    (-1i128).sqrt();
}

// -- try_as --

#[test]
fun test_try_as_i8() {
    assert_eq!(0i128.try_as_i8(), option::some(0i8));
    assert_eq!(127i128.try_as_i8(), option::some(127i8));
    assert_eq!((-128i128).try_as_i8(), option::some(-128i8));
    assert_eq!(128i128.try_as_i8(), option::none());
    assert_eq!((-129i128).try_as_i8(), option::none());
}

#[test]
fun test_try_as_i16() {
    assert_eq!(0i128.try_as_i16(), option::some(0i16));
    assert_eq!(32767i128.try_as_i16(), option::some(32767i16));
    assert_eq!((-32768i128).try_as_i16(), option::some(-32768i16));
    assert_eq!(32768i128.try_as_i16(), option::none());
    assert_eq!((-32769i128).try_as_i16(), option::none());
}

#[test]
fun test_try_as_i32() {
    assert_eq!(0i128.try_as_i32(), option::some(0i32));
    assert_eq!(2147483647i128.try_as_i32(), option::some(2147483647i32));
    assert_eq!((-2147483648i128).try_as_i32(), option::some(-2147483648i32));
    assert_eq!(2147483648i128.try_as_i32(), option::none());
    assert_eq!((-2147483649i128).try_as_i32(), option::none());
}

#[test]
fun test_try_as_i64() {
    assert_eq!(0i128.try_as_i64(), option::some(0i64));
    assert_eq!(9223372036854775807i128.try_as_i64(), option::some(9223372036854775807i64));
    assert_eq!((-9223372036854775808i128).try_as_i64(), option::some(-9223372036854775808i64));
    assert_eq!(9223372036854775808i128.try_as_i64(), option::none());
    assert_eq!((-9223372036854775809i128).try_as_i64(), option::none());
}

// -- to_string --

#[test]
fun test_to_string() {
    assert_eq!(0i128.to_string(), b"0".to_string());
    assert_eq!(1i128.to_string(), b"1".to_string());
    assert_eq!((-1i128).to_string(), b"-1".to_string());
    assert_eq!(MAX.to_string(), b"170141183460469231731687303715884105727".to_string());
    assert_eq!(MIN.to_string(), b"-170141183460469231731687303715884105728".to_string());
}

// -- do / do_eq --

#[test]
fun test_do() {
    let mut sum: i128 = 0;
    5i128.do!(|i| sum = sum + i);
    assert_eq!(sum, 10);
}

#[test]
fun test_do_eq() {
    let mut sum: i128 = 0;
    5i128.do_eq!(|i| sum = sum + i);
    assert_eq!(sum, 15);
}

#[test]
fun test_do_zero() {
    0i128.do!(|_| assert!(false));
}

// -- range_do / range_do_eq --

#[test]
fun test_range_do() {
    let mut sum: i128 = 0;
    (-3i128).range_do!(3i128, |i| sum = sum + i);
    assert_eq!(sum, -3);
}

#[test]
fun test_range_do_eq() {
    let mut sum: i128 = 0;
    (-3i128).range_do_eq!(3i128, |i| sum = sum + i);
    assert_eq!(sum, 0);
}

#[test]
fun test_range_do_empty() {
    3i128.range_do!(0i128, |_| assert!(false));
    3i128.range_do_eq!(0i128, |_| assert!(false));
}
