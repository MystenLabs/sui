// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[defines_primitive(i8)]
module std::i8;

use std::string::String;

/// Maximum value for an `i8`
public macro fun max_value(): i8 {
    127i8
}

/// Minimum value for an `i8`
public macro fun min_value(): i8 {
    -128i8
}

/// Return the larger of `x` and `y`
public fun max(x: i8, y: i8): i8 {
    std::macros::num_max!(x, y)
}

/// Return the smaller of `x` and `y`
public fun min(x: i8, y: i8): i8 {
    std::macros::num_min!(x, y)
}

/// Return the absolute value of `x` as the unsigned counterpart type.
/// This is safe for all values including `MIN`.
public fun abs(x: i8): u8 {
    if (x >= 0) (x as u8)
    else {
        let pos = -(x + 1);
        (pos as u8) + 1
    }
}

/// Return the sign of `x`: -1 if negative, 0 if zero, 1 if positive.
public fun sign(x: i8): i8 {
    if (x > 0) 1
    else if (x == 0) 0
    else -1
}

/// Return the value of a base raised to a power
public fun pow(base: i8, exponent: u8): i8 {
    std::macros::num_pow!(base, exponent)
}

/// Get a nearest lower integer Square Root for `x`. Aborts if `x` is negative.
public fun sqrt(x: i8): i8 {
    assert!(x >= 0);
    std::macros::num_sqrt!<i8, i16>(x, 8)
}

public fun to_string(x: i8): String {
    std::macros::signed_num_to_string!(x)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (exclusive)
public macro fun range_do<$R: drop>($start: i8, $stop: i8, $f: |i8| -> $R) {
    std::macros::range_do!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (inclusive)
public macro fun range_do_eq<$R: drop>($start: i8, $stop: i8, $f: |i8| -> $R) {
    std::macros::range_do_eq!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (exclusive)
public macro fun do<$R: drop>($stop: i8, $f: |i8| -> $R) {
    std::macros::do!($stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (inclusive)
public macro fun do_eq<$R: drop>($stop: i8, $f: |i8| -> $R) {
    std::macros::do_eq!($stop, $f)
}
