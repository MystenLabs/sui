// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::i256_tests;

use std::unit_test::assert_eq;

const MAX: i256 = 57896044618658097711785492504343953926634992332820282019728792003956564819967;
const MIN: i256 = -57896044618658097711785492504343953926634992332820282019728792003956564819968;

// -- max / min --

#[test]
fun test_max() {
    assert_eq!(5i256.max(10i256), 10);
    assert_eq!((-5i256).max(5i256), 5);
    assert_eq!((-100i256).max(-50i256), -50);
    assert_eq!(MAX.max(MIN), MAX);
}

#[test]
fun test_min() {
    assert_eq!(5i256.min(10i256), 5);
    assert_eq!((-5i256).min(5i256), -5);
    assert_eq!((-100i256).min(-50i256), -100);
    assert_eq!(MAX.min(MIN), MIN);
}

// -- abs --

#[test]
fun test_abs() {
    assert_eq!(0i256.abs(), 0u256);
    assert_eq!(42i256.abs(), 42u256);
    assert_eq!((-42i256).abs(), 42u256);
    assert_eq!(
        MAX.abs(),
        57896044618658097711785492504343953926634992332820282019728792003956564819967u256,
    );
    assert_eq!(
        MIN.abs(),
        57896044618658097711785492504343953926634992332820282019728792003956564819968u256,
    );
}

// -- sign --

#[test]
fun test_sign() {
    assert_eq!(42i256.sign(), 1);
    assert_eq!(0i256.sign(), 0);
    assert_eq!((-42i256).sign(), -1);
    assert_eq!(MAX.sign(), 1);
    assert_eq!(MIN.sign(), -1);
}

// -- pow --

#[test]
fun test_pow() {
    assert_eq!(2i256.pow(0), 1);
    assert_eq!(2i256.pow(10), 1024);
    assert_eq!((-2i256).pow(3), -8);
    assert_eq!((-1i256).pow(0), 1);
    assert_eq!((-1i256).pow(1), -1);
    assert_eq!((-1i256).pow(2), 1);
    assert_eq!(3i256.pow(5), 243);
    assert_eq!(0i256.pow(0), 1);
}

#[test, expected_failure(arithmetic_error, location = std::i256)]
fun test_pow_overflow() {
    2i256.pow(255);
}

// -- try_as --

#[test]
fun test_try_as_i8() {
    assert_eq!(0i256.try_as_i8(), option::some(0i8));
    assert_eq!(127i256.try_as_i8(), option::some(127i8));
    assert_eq!((-128i256).try_as_i8(), option::some(-128i8));
    assert_eq!(128i256.try_as_i8(), option::none());
    assert_eq!((-129i256).try_as_i8(), option::none());
}

#[test]
fun test_try_as_i16() {
    assert_eq!(0i256.try_as_i16(), option::some(0i16));
    assert_eq!(32767i256.try_as_i16(), option::some(32767i16));
    assert_eq!((-32768i256).try_as_i16(), option::some(-32768i16));
    assert_eq!(32768i256.try_as_i16(), option::none());
    assert_eq!((-32769i256).try_as_i16(), option::none());
}

#[test]
fun test_try_as_i32() {
    assert_eq!(0i256.try_as_i32(), option::some(0i32));
    assert_eq!(2147483647i256.try_as_i32(), option::some(2147483647i32));
    assert_eq!((-2147483648i256).try_as_i32(), option::some(-2147483648i32));
    assert_eq!(2147483648i256.try_as_i32(), option::none());
    assert_eq!((-2147483649i256).try_as_i32(), option::none());
}

#[test]
fun test_try_as_i64() {
    assert_eq!(0i256.try_as_i64(), option::some(0i64));
    assert_eq!(9223372036854775807i256.try_as_i64(), option::some(9223372036854775807i64));
    assert_eq!((-9223372036854775808i256).try_as_i64(), option::some(-9223372036854775808i64));
    assert_eq!(9223372036854775808i256.try_as_i64(), option::none());
    assert_eq!((-9223372036854775809i256).try_as_i64(), option::none());
}

#[test]
fun test_try_as_i128() {
    assert_eq!(0i256.try_as_i128(), option::some(0i128));
    assert_eq!(
        170141183460469231731687303715884105727i256.try_as_i128(),
        option::some(170141183460469231731687303715884105727i128),
    );
    assert_eq!(
        (-170141183460469231731687303715884105728i256).try_as_i128(),
        option::some(-170141183460469231731687303715884105728i128),
    );
    assert_eq!(170141183460469231731687303715884105728i256.try_as_i128(), option::none());
    assert_eq!((-170141183460469231731687303715884105729i256).try_as_i128(), option::none());
}

// -- to_string --

#[test]
fun test_to_string() {
    assert_eq!(0i256.to_string(), b"0".to_string());
    assert_eq!(1i256.to_string(), b"1".to_string());
    assert_eq!((-1i256).to_string(), b"-1".to_string());
    assert_eq!(
        MAX.to_string(),
        b"57896044618658097711785492504343953926634992332820282019728792003956564819967".to_string(),
    );
    assert_eq!(
        MIN.to_string(),
        b"-57896044618658097711785492504343953926634992332820282019728792003956564819968".to_string(),
    );
}

// -- do / do_eq --

#[test]
fun test_do() {
    let mut sum: i256 = 0;
    5i256.do!(|i| sum = sum + i);
    assert_eq!(sum, 10);
}

#[test]
fun test_do_eq() {
    let mut sum: i256 = 0;
    5i256.do_eq!(|i| sum = sum + i);
    assert_eq!(sum, 15);
}

#[test]
fun test_do_zero() {
    0i256.do!(|_| assert!(false));
}

// -- range_do / range_do_eq --

#[test]
fun test_range_do() {
    let mut sum: i256 = 0;
    (-3i256).range_do!(3i256, |i| sum = sum + i);
    assert_eq!(sum, -3);
}

#[test]
fun test_range_do_eq() {
    let mut sum: i256 = 0;
    (-3i256).range_do_eq!(3i256, |i| sum = sum + i);
    assert_eq!(sum, 0);
}

#[test]
fun test_range_do_empty() {
    3i256.range_do!(0i256, |_| assert!(false));
    3i256.range_do_eq!(0i256, |_| assert!(false));
}
