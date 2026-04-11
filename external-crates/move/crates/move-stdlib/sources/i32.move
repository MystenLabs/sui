// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[defines_primitive(i32)]
module std::i32;

use std::string::String;

/// Maximum value for an `i32`
public macro fun max_value(): i32 {
    2147483647i32
}

/// Minimum value for an `i32`
public macro fun min_value(): i32 {
    -2147483648i32
}

/// Return the larger of `x` and `y`
public fun max(x: i32, y: i32): i32 {
    std::macros::num_max!(x, y)
}

/// Return the smaller of `x` and `y`
public fun min(x: i32, y: i32): i32 {
    std::macros::num_min!(x, y)
}

/// Return the absolute value of `x` as the unsigned counterpart type.
/// This is safe for all values including `MIN`.
public fun abs(x: i32): u32 {
    if (x >= 0) (x as u32)
    else {
        let pos = -(x + 1);
        (pos as u32) + 1
    }
}

/// Return the sign of `x`: -1 if negative, 0 if zero, 1 if positive.
public fun sign(x: i32): i32 {
    if (x > 0) 1
    else if (x == 0) 0
    else -1
}

/// Return the value of a base raised to a power
public fun pow(base: i32, exponent: u8): i32 {
    std::macros::num_pow!(base, exponent)
}

/// Get a nearest lower integer Square Root for `x`. Aborts if `x` is negative.
public fun sqrt(x: i32): i32 {
    assert!(x >= 0);
    std::macros::num_sqrt!<i32, i64>(x, 32)
}

/// Try to convert an `i32` to an `i8`. Returns `None` if the value is out of range.
public fun try_as_i8(x: i32): Option<i8> {
    std::macros::try_as_i8!(x)
}

/// Try to convert an `i32` to an `i16`. Returns `None` if the value is out of range.
public fun try_as_i16(x: i32): Option<i16> {
    std::macros::try_as_i16!(x)
}

public fun to_string(x: i32): String {
    std::macros::signed_num_to_string!(x)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (exclusive)
public macro fun range_do<$R: drop>($start: i32, $stop: i32, $f: |i32| -> $R) {
    std::macros::range_do!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (inclusive)
public macro fun range_do_eq<$R: drop>($start: i32, $stop: i32, $f: |i32| -> $R) {
    std::macros::range_do_eq!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (exclusive)
public macro fun do<$R: drop>($stop: i32, $f: |i32| -> $R) {
    std::macros::do!($stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (inclusive)
public macro fun do_eq<$R: drop>($stop: i32, $f: |i32| -> $R) {
    std::macros::do_eq!($stop, $f)
}
