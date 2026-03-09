// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[defines_primitive(u16)]
module std::u16;

use std::string::String;

/// Returns the bitwise not of the value.
/// Each bit that is 1 becomes 0. Each bit that is 0 becomes 1.
public fun bitwise_not(x: u16): u16 {
    x ^ max_value!()
}

/// Return the larger of `x` and `y`
public fun max(x: u16, y: u16): u16 {
    std::macros::num_max!(x, y)
}

/// Return the smaller of `x` and `y`
public fun min(x: u16, y: u16): u16 {
    std::macros::num_min!(x, y)
}

/// Return the absolute value of x - y
public fun diff(x: u16, y: u16): u16 {
    std::macros::num_diff!(x, y)
}

/// Calculate x / y, but round up the result.
public fun divide_and_round_up(x: u16, y: u16): u16 {
    std::macros::num_divide_and_round_up!(x, y)
}

/// Return the value of a base raised to a power
public fun pow(base: u16, exponent: u8): u16 {
    std::macros::num_pow!(base, exponent)
}

/// Get a nearest lower integer Square Root for `x`. Given that this
/// function can only operate with integers, it is impossible
/// to get perfect (or precise) integer square root for some numbers.
///
/// Example:
/// ```
/// math::sqrt(9) => 3
/// math::sqrt(8) => 2 // the nearest lower square root is 4;
/// ```
///
/// In integer math, one of the possible ways to get results with more
/// precision is to use higher values or temporarily multiply the
/// value by some bigger number. Ideally if this is a square of 10 or 100.
///
/// Example:
/// ```
/// math::sqrt(8) => 2;
/// math::sqrt(8 * 10000) => 282;
/// // now we can use this value as if it was 2.82;
/// // but to get the actual result, this value needs
/// // to be divided by 100 (because sqrt(10000)).
///
///
/// math::sqrt(8 * 1000000) => 2828; // same as above, 2828 / 1000 (2.828)
/// ```
public fun sqrt(x: u16): u16 {
    std::macros::num_sqrt!<u16, u32>(x, 16)
}

/// Try to convert a `u16` to a `u8`. Returns `None` if the value is too large.
public fun try_as_u8(x: u16): Option<u8> {
    std::macros::try_as_u8!(x)
}

public fun to_string(x: u16): String {
    std::macros::num_to_string!(x)
}

/// Try to add `x` and `y`.
/// Returns `None` if the addition would overflow.
public fun checked_add(x: u16, y: u16): Option<u16> {
    std::macros::num_checked_add!(x, y, max_value!())
}

/// Try to subtract `y` from `x`.
/// Returns `None` if `y > x`.
public fun checked_sub(x: u16, y: u16): Option<u16> {
    std::macros::num_checked_sub!(x, y)
}

/// Try to multiply `x` and `y`.
/// Returns `None` if the multiplication would overflow.
public fun checked_mul(x: u16, y: u16): Option<u16> {
    std::macros::num_checked_mul!(x, y, max_value!())
}

/// Try to divide `x` by `y`.
/// Returns `None` if `y` is zero.
public fun checked_div(x: u16, y: u16): Option<u16> {
    std::macros::num_checked_div!(x, y)
}

/// Add `x` and `y`, saturating at the maximum value instead of overflowing.
public fun saturating_add(x: u16, y: u16): u16 {
    std::macros::num_saturating_add!(x, y, max_value!())
}

/// Subtract `y` from `x`, saturating at `0` instead of underflowing.
public fun saturating_sub(x: u16, y: u16): u16 {
    std::macros::num_saturating_sub!(x, y)
}

/// Multiply `x` and `y`, saturating at the maximum value instead of overflowing.
public fun saturating_mul(x: u16, y: u16): u16 {
    std::macros::num_saturating_mul!(x, y, max_value!())
}

/// Shifts `x` left by `shift` bits.
/// Returns `None` if the shift is greater than or equal to the bit size of 16.
public fun checked_shl(x: u16, shift: u8): Option<u16> {
    std::macros::num_checked_shl!(x, shift, 16)
}

/// Shifts `x` right by `shift` bits.
/// Returns `None` if the shift is greater than or equal to the bit size of 16.
public fun checked_shr(x: u16, shift: u8): Option<u16> {
    std::macros::num_checked_shr!(x, shift, 16)
}

/// Shifts `x` left by `shift` bits.
/// Returns `None` if the shift is larger than or equal to the bit size of 16, or if the shift would
/// lose any bits (if the operation is not reversible).
public fun lossless_shl(x: u16, shift: u8): Option<u16> {
    std::macros::num_lossless_shl!(x, shift, 16)
}

/// Shifts `x` right by `shift` bits.
/// Returns `None` if the shift is larger than or equal to the bit size of 16, or if the shift would
/// lose any bits (if the operation is not reversible).
public fun lossless_shr(x: u16, shift: u8): Option<u16> {
    std::macros::num_lossless_shr!(x, shift, 16)
}

/// Divides `x` by `y`.
/// Returns `None` if `y` is zero or if there is a non-zero remainder (if `x % y != 0`). In other
/// words, it returns `None` if the operation is not reversible.
public fun lossless_div(x: u16, y: u16): Option<u16> {
    std::macros::num_lossless_div!(x, y)
}

/// Maximum value for a `u16`
public macro fun max_value(): u16 {
    0xFFFF
}

/// Loops applying `$f` to each number from `$start` to `$stop` (exclusive)
public macro fun range_do<$R: drop>($start: u16, $stop: u16, $f: |u16| -> $R) {
    std::macros::range_do!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `$start` to `$stop` (inclusive)
public macro fun range_do_eq<$R: drop>($start: u16, $stop: u16, $f: |u16| -> $R) {
    std::macros::range_do_eq!($start, $stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (exclusive)
public macro fun do<$R: drop>($stop: u16, $f: |u16| -> $R) {
    std::macros::do!($stop, $f)
}

/// Loops applying `$f` to each number from `0` to `$stop` (inclusive)
public macro fun do_eq<$R: drop>($stop: u16, $f: |u16| -> $R) {
    std::macros::do_eq!($stop, $f)
}
