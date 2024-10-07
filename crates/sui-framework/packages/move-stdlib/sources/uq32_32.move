// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines an unisnged, fixed-point numeric type with a 32-bit integer part and a 32-bit fractional
/// part. The notation `uq32_32` and `UQ32_32` is based on
/// [Q notation](https://en.wikipedia.org/wiki/Q_(number_format)).`q` indicates it a
/// fixed-point number number. The `u` prefix indicates it is unsigned. The `32_32` suffix indicates
/// number of bits, where the first number indicates the number of bits in the integer part,
/// and the second the number of bits in the fractional part--in this case 32 bits for each.
module std::uq32_32;

#[error]
const EDenominator: vector<u8> = b"`from_rational` called with a denominator of zero";

#[error]
const ERatioTooSmall: vector<u8> =
    b"`from_rational` called with a ratio that is too small, and is outside of the supported range";

#[error]
const ERatioTooLarge: vector<u8> =
    b"`from_rational` called with a ratio that is too large, and is outside of the supported range";

#[error]
const EOverflow: vector<u8> = b"Overflow from an arithmetic operation";

#[error]
const EDivisionByZero: vector<u8> = b"Division by zero";

/// A fixed-point numeric type with 32 integer bits nd 32 fractional bits, represented by an
/// underlying 64 value. This is a binary representation, so decimal values may not be exactly
/// representable, but it provides more than 9 decimal digits of precision both before and after the
/// decimal point (18 digits total).
public struct UQ32_32(u64) has copy, drop, store;

/// Create a fixed-point value from a rational number specified by its numerator and denominator.
/// `from_rational` and `from_integer` should be preferred over using `from_raw`.
/// When specifying decimal fractions, be careful about rounding errors. If you round to display
/// N digits after the decimal point, you can use a denominator of 10^N to avoid numbers where the
/// very small imprecision in the binary representation could change the rounding. For example,
/// `0.0125` will round down to `0.012` instead of up to `0.013`.
/// Aborts if the denominator is zero.
/// Aborts if the numerator is non-zero and the ratio is not in the range 2^-32 .. 2^32-1.
public fun from_rational(numerator: u64, denominator: u64): UQ32_32 {
    // If the denominator is zero, this will abort.
    // Scale the numerator to have 64 fractional bits and the denominator to have 32 fractional
    // bits, so that the quotient will have 32 fractional bits.
    let scaled_numerator = numerator as u128 << 64;
    let scaled_denominator = denominator as u128 << 32;
    assert!(scaled_denominator != 0, EDenominator);
    let quotient = scaled_numerator / scaled_denominator;
    assert!(quotient != 0 || numerator == 0, ERatioTooSmall);
    // Return the quotient as a fixed-point number. We first need to check whether the cast
    // can succeed.
    assert!(quotient <= std::u64::max_value!() as u128, ERatioTooLarge);
    UQ32_32(quotient as u64)
}

/// Create a fixed-point value from an integer.
/// `from_integer` and `from_rational` should be preferred over using `from_raw`.
public fun from_integer(integer: u32): UQ32_32 {
    UQ32_32((integer as u64) << 32)
}

/// Add two fixed-point numbers, `a + b`.
/// Aborts if the sum overflows.
public fun add(a: UQ32_32, b: UQ32_32): UQ32_32 {
    let sum = a.0 as u128 + (b.0 as u128);
    assert!(sum <= std::u64::max_value!() as u128, EOverflow);
    UQ32_32(sum as u64)
}

/// Subtract two fixed-point numbers, `a - b`.
/// Aborts if `a < b`.
public fun sub(a: UQ32_32, b: UQ32_32): UQ32_32 {
    assert!(a.0 >= b.0, EOverflow);
    UQ32_32(a.0 - b.0)
}

// Multiply two fixed-point numbers, truncating any fractional part of the product.
/// Aborts if the product overflows.
public fun mul(a: UQ32_32, b: UQ32_32): UQ32_32 {
    UQ32_32(int_mul(a.0, b))
}

/// Divide two fixed-point numbers, truncating any fractional part of the quotient.
/// Aborts if the divisor is zero.
/// Aborts if the quotient overflows.
public fun div(a: UQ32_32, b: UQ32_32): UQ32_32 {
    UQ32_32(int_div(a.0, b))
}

/// Multiply a `u64` integer by a fixed-point number, truncating any fractional part of the product.
/// Aborts if the product overflows.
public fun int_mul(val: u64, multiplier: UQ32_32): u64 {
    // The product of two 64 bit values has 128 bits, so perform the
    // multiplication with u128 types and keep the full 128 bit product
    // to avoid losing accuracy.
    let unscaled_product = val as u128 * (multiplier.0 as u128);
    // The unscaled product has 32 fractional bits (from the multiplier)
    // so rescale it by shifting away the low bits.
    let product = unscaled_product >> 32;
    // Check whether the value is too large.
    assert!(product <= std::u64::max_value!() as u128, EOverflow);
    product as u64
}

/// Divide a `u64` integer by a fixed-point number, truncating any fractional part of the quotient.
/// Aborts if the divisor is zero.
/// Aborts if the quotient overflows.
public fun int_div(val: u64, divisor: UQ32_32): u64 {
    // Check for division by zero.
    assert!(divisor.0 != 0, EDivisionByZero);
    // First convert to 128 bits and then shift left to
    // add 32 fractional zero bits to the dividend.
    let scaled_value = val as u128 << 32;
    let quotient = scaled_value / (divisor.0 as u128);
    // Check whether the value is too large.
    assert!(quotient <= std::u64::max_value!() as u128, EOverflow);
    // the value may be too large, which will cause the cast to fail
    // with an arithmetic error.
    quotient as u64
}

/// Less than or equal to. Returns `true` if and only if `a <= a`.
public fun le(a: UQ32_32, b: UQ32_32): bool {
    a.0 <= b.0
}

/// Less than. Returns `true` if and only if `a < b`.
public fun lt(a: UQ32_32, b: UQ32_32): bool {
    a.0 < b.0
}

/// Greater than or equal to. Returns `true` if and only if `a >= b`.
public fun ge(a: UQ32_32, b: UQ32_32): bool {
    a.0 >= b.0
}

/// Greater than. Returns `true` if and only if `a > b`.
public fun gt(a: UQ32_32, b: UQ32_32): bool {
    a.0 > b.0
}

/// Accessor for the raw u64 value. Can be paired with `from_raw` to perform less common operations
/// on the raw values directly.
public fun to_raw(a: UQ32_32): u64 {
    a.0
}

/// Accessor for the raw u64 value. Can be paired with `to_raw` to perform less common operations
/// on the raw values directly.
public fun from_raw(raw_value: u64): UQ32_32 {
    UQ32_32(raw_value)
}
