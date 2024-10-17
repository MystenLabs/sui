// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines an unsigned, fixed-point numeric type with a 64-bit integer part and a 64-bit fractional
/// part. The notation `uq64_64` and `UQ64_64` is based on
/// [Q notation](https://en.wikipedia.org/wiki/Q_(number_format)). `q` indicates it a fixed-point
/// number. The `u` prefix indicates it is unsigned. The `64_64` suffix indicates the number of
/// bits, where the first number indicates the number of bits in the integer part, and the second
/// the number of bits in the fractional part--in this case 64 bits for each.
module std::uq64_64;

#[error]
const EDenominator: vector<u8> = b"Quotient specified with a zero denominator";

#[error]
const EQuotientTooSmall: vector<u8> =
    b"Quotient specified is too small, and is outside of the supported range";

#[error]
const EQuotientTooLarge: vector<u8> =
    b"Quotient specified is too large, and is outside of the supported range";

#[error]
const EOverflow: vector<u8> = b"Overflow from an arithmetic operation";

#[error]
const EDivisionByZero: vector<u8> = b"Division by zero";

/// A fixed-point numeric type with 64 integer bits and 64 fractional bits, represented by an
/// underlying 128 bit value. This is a binary representation, so decimal values may not be exactly
/// representable, but it provides more than 9 decimal digits of precision both before and after the
/// decimal point (18 digits total).
public struct UQ64_64(u128) has copy, drop, store;

/// Create a fixed-point value from a quotient specified by its numerator and denominator.
/// `from_quotient` and `from_int` should be preferred over using `from_raw`.
/// Unless the denominator is a power of two, fractions can not be represented accurately,
/// so be careful about rounding errors.
/// Aborts if the denominator is zero.
/// Aborts if the input is non-zero but so small that it will be represented as zero, e.g. smaller
/// than 2^{-64}.
/// Aborts if the input is too large, e.g. larger than or equal to 2^64.
public fun from_quotient(numerator: u128, denominator: u128): UQ64_64 {
    assert!(denominator != 0, EDenominator);

    // Scale the numerator to have 128 fractional bits and the denominator to have 64 fractional
    // bits, so that the quotient will have 64 fractional bits.
    let scaled_numerator = numerator as u256 << 128;
    let scaled_denominator = denominator as u256 << 64;
    let quotient = scaled_numerator / scaled_denominator;

    // The quotient can only be zero if the numerator is also zero.
    assert!(quotient != 0 || numerator == 0, EQuotientTooSmall);

    // Return the quotient as a fixed-point number. We first need to check whether the cast
    // can succeed.
    assert!(quotient <= std::u128::max_value!() as u256, EQuotientTooLarge);
    UQ64_64(quotient as u128)
}

/// Create a fixed-point value from an integer.
/// `from_int` and `from_quotient` should be preferred over using `from_raw`.
public fun from_int(integer: u64): UQ64_64 {
    UQ64_64((integer as u128) << 64)
}

/// Add two fixed-point numbers, `a + b`.
/// Aborts if the sum overflows.
public fun add(a: UQ64_64, b: UQ64_64): UQ64_64 {
    let sum = a.0 as u256 + (b.0 as u256);
    assert!(sum <= std::u128::max_value!() as u256, EOverflow);
    UQ64_64(sum as u128)
}

/// Subtract two fixed-point numbers, `a - b`.
/// Aborts if `a < b`.
public fun sub(a: UQ64_64, b: UQ64_64): UQ64_64 {
    assert!(a.0 >= b.0, EOverflow);
    UQ64_64(a.0 - b.0)
}

/// Multiply two fixed-point numbers, truncating any fractional part of the product.
/// Aborts if the product overflows.
public fun mul(a: UQ64_64, b: UQ64_64): UQ64_64 {
    UQ64_64(int_mul(a.0, b))
}

/// Divide two fixed-point numbers, truncating any fractional part of the quotient.
/// Aborts if the divisor is zero.
/// Aborts if the quotient overflows.
public fun div(a: UQ64_64, b: UQ64_64): UQ64_64 {
    UQ64_64(int_div(a.0, b))
}

/// Convert a fixed-point number to an integer, truncating any fractional part.
public fun to_int(a: UQ64_64): u64 {
    (a.0 >> 64) as u64
}

/// Multiply a `u128` integer by a fixed-point number, truncating any fractional part of the product.
/// Aborts if the product overflows.
public fun int_mul(val: u128, multiplier: UQ64_64): u128 {
    // The product of two 128 bit values has 256 bits, so perform the
    // multiplication with u256 types and keep the full 256 bit product
    // to avoid losing accuracy.
    let unscaled_product = val as u256 * (multiplier.0 as u256);
    // The unscaled product has 64 fractional bits (from the multiplier)
    // so rescale it by shifting away the low bits.
    let product = unscaled_product >> 64;
    // Check whether the value is too large.
    assert!(product <= std::u128::max_value!() as u256, EOverflow);
    product as u128
}

/// Divide a `u128` integer by a fixed-point number, truncating any fractional part of the quotient.
/// Aborts if the divisor is zero.
/// Aborts if the quotient overflows.
public fun int_div(val: u128, divisor: UQ64_64): u128 {
    // Check for division by zero.
    assert!(divisor.0 != 0, EDivisionByZero);
    // First convert to 256 bits and then shift left to
    // add 64 fractional zero bits to the dividend.
    let scaled_value = val as u256 << 64;
    let quotient = scaled_value / (divisor.0 as u256);
    // Check whether the value is too large.
    assert!(quotient <= std::u128::max_value!() as u256, EOverflow);
    quotient as u128
}

/// Less than or equal to. Returns `true` if and only if `a <= a`.
public fun le(a: UQ64_64, b: UQ64_64): bool {
    a.0 <= b.0
}

/// Less than. Returns `true` if and only if `a < b`.
public fun lt(a: UQ64_64, b: UQ64_64): bool {
    a.0 < b.0
}

/// Greater than or equal to. Returns `true` if and only if `a >= b`.
public fun ge(a: UQ64_64, b: UQ64_64): bool {
    a.0 >= b.0
}

/// Greater than. Returns `true` if and only if `a > b`.
public fun gt(a: UQ64_64, b: UQ64_64): bool {
    a.0 > b.0
}

/// Accessor for the raw u128 value. Can be paired with `from_raw` to perform less common operations
/// on the raw values directly.
public fun to_raw(a: UQ64_64): u128 {
    a.0
}

/// Accessor for the raw u128 value. Can be paired with `to_raw` to perform less common operations
/// on the raw values directly.
public fun from_raw(raw_value: u128): UQ64_64 {
    UQ64_64(raw_value)
}
