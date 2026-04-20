// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[defines_primitive(i256)]
module std::i256;

use std::string::String;

/// Maximum value for an `i256`
public macro fun max_value(): i256 {
    57896044618658097711785492504343953926634992332820282019728792003956564819967i256
}

/// Minimum value for an `i256`
public macro fun min_value(): i256 {
    -57896044618658097711785492504343953926634992332820282019728792003956564819968i256
}

/// Return the larger of `x` and `y`
public fun max(x: i256, y: i256): i256 {
    std::macros::num_max!(x, y)
}

/// Return the smaller of `x` and `y`
public fun min(x: i256, y: i256): i256 {
    std::macros::num_min!(x, y)
}

/// Return the absolute value of `x` as the unsigned counterpart type.
/// This is safe for all values including `MIN`.
public fun abs(x: i256): u256 {
    if (x >= 0) (x as u256)
    else {
        let pos = -(x + 1);
        (pos as u256) + 1
    }
}

/// Return the sign of `x`: -1 if negative, 0 if zero, 1 if positive.
public fun sign(x: i256): i256 {
    if (x > 0) 1
    else if (x == 0) 0
    else -1
}

/// Return the value of a base raised to a power
public fun pow(base: i256, exponent: u8): i256 {
    std::macros::num_pow!(base, exponent)
}

/// Try to convert an `i256` to an `i8`. Returns `None` if the value is out of range.
public fun try_as_i8(x: i256): Option<i8> {
    std::macros::try_as_i8!(x)
}

/// Try to convert an `i256` to an `i16`. Returns `None` if the value is out of range.
public fun try_as_i16(x: i256): Option<i16> {
    std::macros::try_as_i16!(x)
}

/// Try to convert an `i256` to an `i32`. Returns `None` if the value is out of range.
public fun try_as_i32(x: i256): Option<i32> {
    std::macros::try_as_i32!(x)
}

/// Try to convert an `i256` to an `i64`. Returns `None` if the value is out of range.
public fun try_as_i64(x: i256): Option<i64> {
    std::macros::try_as_i64!(x)
}

/// Try to convert an `i256` to an `i128`. Returns `None` if the value is out of range.
public fun try_as_i128(x: i256): Option<i128> {
    std::macros::try_as_i128!(x)
}

public fun to_string(x: i256): String {
    std::macros::signed_num_to_string!(x)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (exclusive)
public macro fun range_do<$R: drop>($start: i256, $stop: i256, $f: |i256| -> $R) {
    std::macros::range_do!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (inclusive)
public macro fun range_do_eq<$R: drop>($start: i256, $stop: i256, $f: |i256| -> $R) {
    std::macros::range_do_eq!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (exclusive)
public macro fun do<$R: drop>($stop: i256, $f: |i256| -> $R) {
    std::macros::do!($stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (inclusive)
public macro fun do_eq<$R: drop>($stop: i256, $f: |i256| -> $R) {
    std::macros::do_eq!($stop, $f)
}
