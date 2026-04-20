// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[defines_primitive(i64)]
module std::i64;

use std::string::String;

/// Maximum value for an `i64`
public macro fun max_value(): i64 {
    9223372036854775807i64
}

/// Minimum value for an `i64`
public macro fun min_value(): i64 {
    -9223372036854775808i64
}

/// Return the larger of `x` and `y`
public fun max(x: i64, y: i64): i64 {
    std::macros::num_max!(x, y)
}

/// Return the smaller of `x` and `y`
public fun min(x: i64, y: i64): i64 {
    std::macros::num_min!(x, y)
}

/// Return the absolute value of `x` as the unsigned counterpart type.
/// This is safe for all values including `MIN`.
public fun abs(x: i64): u64 {
    if (x >= 0) (x as u64)
    else {
        let pos = -(x + 1);
        (pos as u64) + 1
    }
}

/// Return the sign of `x`: -1 if negative, 0 if zero, 1 if positive.
public fun sign(x: i64): i64 {
    if (x > 0) 1
    else if (x == 0) 0
    else -1
}

/// Return the value of a base raised to a power
public fun pow(base: i64, exponent: u8): i64 {
    std::macros::num_pow!(base, exponent)
}

/// Get a nearest lower integer Square Root for `x`. Aborts if `x` is negative.
public fun sqrt(x: i64): i64 {
    assert!(x >= 0);
    std::macros::num_sqrt!<i64, i128>(x, 64)
}

/// Try to convert an `i64` to an `i8`. Returns `None` if the value is out of range.
public fun try_as_i8(x: i64): Option<i8> {
    std::macros::try_as_i8!(x)
}

/// Try to convert an `i64` to an `i16`. Returns `None` if the value is out of range.
public fun try_as_i16(x: i64): Option<i16> {
    std::macros::try_as_i16!(x)
}

/// Try to convert an `i64` to an `i32`. Returns `None` if the value is out of range.
public fun try_as_i32(x: i64): Option<i32> {
    std::macros::try_as_i32!(x)
}

public fun to_string(x: i64): String {
    std::macros::signed_num_to_string!(x)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (exclusive)
public macro fun range_do<$R: drop>($start: i64, $stop: i64, $f: |i64| -> $R) {
    std::macros::range_do!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (inclusive)
public macro fun range_do_eq<$R: drop>($start: i64, $stop: i64, $f: |i64| -> $R) {
    std::macros::range_do_eq!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (exclusive)
public macro fun do<$R: drop>($stop: i64, $f: |i64| -> $R) {
    std::macros::do!($stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (inclusive)
public macro fun do_eq<$R: drop>($stop: i64, $f: |i64| -> $R) {
    std::macros::do_eq!($stop, $f)
}
