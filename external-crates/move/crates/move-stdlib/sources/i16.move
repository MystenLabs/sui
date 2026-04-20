// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[defines_primitive(i16)]
module std::i16;

use std::string::String;

/// Maximum value for an `i16`
public macro fun max_value(): i16 {
    32767i16
}

/// Minimum value for an `i16`
public macro fun min_value(): i16 {
    -32768i16
}

/// Return the larger of `x` and `y`
public fun max(x: i16, y: i16): i16 {
    std::macros::num_max!(x, y)
}

/// Return the smaller of `x` and `y`
public fun min(x: i16, y: i16): i16 {
    std::macros::num_min!(x, y)
}

/// Return the absolute value of `x` as the unsigned counterpart type.
/// This is safe for all values including `MIN`.
public fun abs(x: i16): u16 {
    if (x >= 0) (x as u16)
    else {
        let pos = -(x + 1);
        (pos as u16) + 1
    }
}

/// Return the sign of `x`: -1 if negative, 0 if zero, 1 if positive.
public fun sign(x: i16): i16 {
    if (x > 0) 1
    else if (x == 0) 0
    else -1
}

/// Return the value of a base raised to a power
public fun pow(base: i16, exponent: u8): i16 {
    std::macros::num_pow!(base, exponent)
}

/// Get a nearest lower integer Square Root for `x`. Aborts if `x` is negative.
public fun sqrt(x: i16): i16 {
    assert!(x >= 0);
    std::macros::num_sqrt!<i16, i32>(x, 16)
}

/// Try to convert an `i16` to an `i8`. Returns `None` if the value is out of range.
public fun try_as_i8(x: i16): Option<i8> {
    std::macros::try_as_i8!(x)
}

public fun to_string(x: i16): String {
    std::macros::signed_num_to_string!(x)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (exclusive)
public macro fun range_do<$R: drop>($start: i16, $stop: i16, $f: |i16| -> $R) {
    std::macros::range_do!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (inclusive)
public macro fun range_do_eq<$R: drop>($start: i16, $stop: i16, $f: |i16| -> $R) {
    std::macros::range_do_eq!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (exclusive)
public macro fun do<$R: drop>($stop: i16, $f: |i16| -> $R) {
    std::macros::do!($stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (inclusive)
public macro fun do_eq<$R: drop>($stop: i16, $f: |i16| -> $R) {
    std::macros::do_eq!($stop, $f)
}
