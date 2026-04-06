// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::i64_tests;

use std::unit_test::assert_eq;

// -- max / min --

#[test]
fun test_max() {
    assert_eq!(5i64.max(10i64), 10);
    assert_eq!((-5i64).max(5i64), 5);
    assert_eq!((-100i64).max(-50i64), -50);
    assert_eq!(9223372036854775807i64.max(-9223372036854775808i64), 9223372036854775807);
}

#[test]
fun test_min() {
    assert_eq!(5i64.min(10i64), 5);
    assert_eq!((-5i64).min(5i64), -5);
    assert_eq!((-100i64).min(-50i64), -100);
    assert_eq!(9223372036854775807i64.min(-9223372036854775808i64), -9223372036854775808);
}

// -- abs --

#[test]
fun test_abs() {
    assert_eq!(0i64.abs(), 0u64);
    assert_eq!(42i64.abs(), 42u64);
    assert_eq!((-42i64).abs(), 42u64);
    assert_eq!(9223372036854775807i64.abs(), 9223372036854775807u64);
    assert_eq!((-9223372036854775807i64).abs(), 9223372036854775807u64);
    assert_eq!((-9223372036854775808i64).abs(), 9223372036854775808u64);
}

// -- sign --

#[test]
fun test_sign() {
    assert_eq!(42i64.sign(), 1);
    assert_eq!(0i64.sign(), 0);
    assert_eq!((-42i64).sign(), -1);
    assert_eq!(9223372036854775807i64.sign(), 1);
    assert_eq!((-9223372036854775808i64).sign(), -1);
}

// -- pow --

#[test]
fun test_pow() {
    assert_eq!(2i64.pow(0), 1);
    assert_eq!(2i64.pow(62), 4611686018427387904);
    assert_eq!((-2i64).pow(3), -8);
    assert_eq!((-1i64).pow(0), 1);
    assert_eq!((-1i64).pow(1), -1);
    assert_eq!((-1i64).pow(2), 1);
    assert_eq!(3i64.pow(5), 243);
    assert_eq!(0i64.pow(0), 1);
}

#[test, expected_failure(arithmetic_error, location = std::i64)]
fun test_pow_overflow() {
    2i64.pow(63);
}

// -- sqrt --

#[test]
fun test_sqrt() {
    assert_eq!(0i64.sqrt(), 0);
    assert_eq!(1i64.sqrt(), 1);
    assert_eq!(4i64.sqrt(), 2);
    assert_eq!(9i64.sqrt(), 3);
    assert_eq!(100i64.sqrt(), 10);
    assert_eq!(10000i64.sqrt(), 100);
    assert_eq!(15i64.sqrt(), 3);
}

#[test, expected_failure]
fun test_sqrt_negative() {
    (-1i64).sqrt();
}

// -- try_as --

#[test]
fun test_try_as_i8() {
    assert_eq!(0i64.try_as_i8(), option::some(0i8));
    assert_eq!(127i64.try_as_i8(), option::some(127i8));
    assert_eq!((-128i64).try_as_i8(), option::some(-128i8));
    assert_eq!(128i64.try_as_i8(), option::none());
    assert_eq!((-129i64).try_as_i8(), option::none());
}

#[test]
fun test_try_as_i16() {
    assert_eq!(0i64.try_as_i16(), option::some(0i16));
    assert_eq!(32767i64.try_as_i16(), option::some(32767i16));
    assert_eq!((-32768i64).try_as_i16(), option::some(-32768i16));
    assert_eq!(32768i64.try_as_i16(), option::none());
    assert_eq!((-32769i64).try_as_i16(), option::none());
}

#[test]
fun test_try_as_i32() {
    assert_eq!(0i64.try_as_i32(), option::some(0i32));
    assert_eq!(2147483647i64.try_as_i32(), option::some(2147483647i32));
    assert_eq!((-2147483648i64).try_as_i32(), option::some(-2147483648i32));
    assert_eq!(2147483648i64.try_as_i32(), option::none());
    assert_eq!((-2147483649i64).try_as_i32(), option::none());
}

// -- to_string --

#[test]
fun test_to_string() {
    assert_eq!(0i64.to_string(), b"0".to_string());
    assert_eq!(1i64.to_string(), b"1".to_string());
    assert_eq!((-1i64).to_string(), b"-1".to_string());
    assert_eq!(9223372036854775807i64.to_string(), b"9223372036854775807".to_string());
    assert_eq!((-9223372036854775808i64).to_string(), b"-9223372036854775808".to_string());
}

// -- do / do_eq --

#[test]
fun test_do() {
    let mut sum: i64 = 0;
    5i64.do!(|i| sum = sum + i);
    assert_eq!(sum, 10);
}

#[test]
fun test_do_eq() {
    let mut sum: i64 = 0;
    5i64.do_eq!(|i| sum = sum + i);
    assert_eq!(sum, 15);
}

#[test]
fun test_do_zero() {
    0i64.do!(|_| assert!(false));
}

// -- range_do / range_do_eq --

#[test]
fun test_range_do() {
    let mut sum: i64 = 0;
    (-3i64).range_do!(3i64, |i| sum = sum + i);
    assert_eq!(sum, -3);
}

#[test]
fun test_range_do_eq() {
    let mut sum: i64 = 0;
    (-3i64).range_do_eq!(3i64, |i| sum = sum + i);
    assert_eq!(sum, 0);
}

#[test]
fun test_range_do_empty() {
    3i64.range_do!(0i64, |_| assert!(false));
    3i64.range_do_eq!(0i64, |_| assert!(false));
}
