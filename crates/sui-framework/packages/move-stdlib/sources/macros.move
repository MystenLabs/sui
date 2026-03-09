// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module holds shared implementation of macros used in `std`
module std::macros;

use std::string::String;

public(package) macro fun num_max<$T>($x: $T, $y: $T): $T {
    let x = $x;
    let y = $y;
    if (x > y) x else y
}

public(package) macro fun num_min<$T>($x: $T, $y: $T): $T {
    let x = $x;
    let y = $y;
    if (x < y) x else y
}

public(package) macro fun num_diff<$T>($x: $T, $y: $T): $T {
    let x = $x;
    let y = $y;
    if (x > y) x - y else y - x
}

public(package) macro fun num_divide_and_round_up<$T>($x: $T, $y: $T): $T {
    let x = $x;
    let y = $y;
    if (x % y == 0) x / y else x / y + 1
}

public(package) macro fun num_pow($base: _, $exponent: u8): _ {
    let mut base = $base;
    let mut exponent = $exponent;
    let mut res = 1;
    while (exponent >= 1) {
        if (exponent % 2 == 0) {
            base = base * base;
            exponent = exponent / 2;
        } else {
            res = res * base;
            exponent = exponent - 1;
        }
    };

    res
}

public(package) macro fun num_sqrt<$T, $U>($x: $T, $bitsize: u8): $T {
    let x = $x;
    let mut bit = (1: $U) << $bitsize;
    let mut res = (0: $U);
    let mut x = x as $U;

    while (bit != 0) {
        if (x >= res + bit) {
            x = x - (res + bit);
            res = (res >> 1) + bit;
        } else {
            res = res >> 1;
        };
        bit = bit >> 2;
    };

    res as $T
}

public(package) macro fun num_to_string($x: _): String {
    let mut x = $x;
    if (x == 0) {
        return b"0".to_string()
    };
    let mut buffer = vector[];
    while (x != 0) {
        buffer.push_back(((48 + x % 10) as u8));
        x = x / 10;
    };
    buffer.reverse();
    buffer.to_string()
}

public(package) macro fun num_checked_add<$T>($x: $T, $y: $T, $max_t: $T): Option<$T> {
    let x = $x;
    let y = $y;
    let max_t = $max_t;
    if (y > max_t - x) option::none() else option::some(x + y)
}

public(package) macro fun num_checked_sub<$T>($x: $T, $y: $T): Option<$T> {
    let x = $x;
    let y = $y;
    if (x < y) option::none() else option::some(x - y)
}

public(package) macro fun num_checked_mul<$T>($x: $T, $y: $T, $max_t: $T): Option<$T> {
    let x = $x;
    let y = $y;
    let max_t = $max_t;
    if (x == 0 || y == 0) option::some(0)
    else if (y > max_t / x) option::none()
    else option::some(x * y)
}

public(package) macro fun num_checked_div<$T>($x: $T, $y: $T): Option<$T> {
    let x = $x;
    let y = $y;
    if (y == 0) option::none() else option::some(x / y)
}

public(package) macro fun num_saturating_add<$T>($x: $T, $y: $T, $max_t: $T): $T {
    let x = $x;
    let y = $y;
    let max_t = $max_t;
    if (y > max_t - x) max_t else x + y
}

public(package) macro fun num_saturating_sub<$T>($x: $T, $y: $T): $T {
    let x = $x;
    let y = $y;
    if (x < y) 0 else x - y
}

public(package) macro fun num_saturating_mul<$T>($x: $T, $y: $T, $max_t: $T): $T {
    let x = $x;
    let y = $y;
    let max_t = $max_t;
    if (x == 0 || y == 0) 0
    else if (y > max_t / x) max_t
    else x * y
}

public macro fun num_checked_shl<$T>($x: $T, $shift: u8, $bit_size: u8): Option<$T> {
    let x = $x;
    let shift = $shift;
    let bit_size = $bit_size;
    if (shift >= bit_size) option::none() else option::some(x << shift)
}

public macro fun num_checked_shr<$T>($x: $T, $shift: u8, $bit_size: u8): Option<$T> {
    let x = $x;
    let shift = $shift;
    let bit_size = $bit_size;
    if (shift >= bit_size) option::none() else option::some(x >> shift)
}

public macro fun num_lossless_shl<$T>($x: $T, $shift: u8, $bit_size: u8): Option<$T> {
    let x = $x;
    let shift = $shift;
    let bit_size = $bit_size;
    if (shift >= bit_size) option::none()
    else {
        let result = x << shift;
        if (result >> shift == x) option::some(result) else option::none()
    }
}

public macro fun num_lossless_shr<$T>($x: $T, $shift: u8, $bit_size: u8): Option<$T> {
    let x = $x;
    let shift = $shift;
    let bit_size = $bit_size;
    if (shift >= bit_size) option::none()
    else {
        let result = x >> shift;
        if (result << shift == x) option::some(result) else option::none()
    }
}

public macro fun num_lossless_div<$T>($x: $T, $y: $T): Option<$T> {
    let x = $x;
    let y = $y;
    if (y == 0) option::none()
    else if (x % y == 0) option::some(x / y)
    else option::none()
}

public macro fun range_do<$T, $R: drop>($start: $T, $stop: $T, $f: |$T| -> $R) {
    let mut i = $start;
    let stop = $stop;
    while (i < stop) {
        $f(i);
        i = i + 1;
    }
}

public macro fun range_do_eq<$T, $R: drop>($start: $T, $stop: $T, $f: |$T| -> $R) {
    let mut i = $start;
    let stop = $stop;
    // we check `i >= stop` inside the loop instead of `i <= stop` as `while` condition to avoid
    // incrementing `i` past the MAX integer value.
    // Because of this, we need to check if `i > stop` and return early--instead of letting the
    // loop bound handle it, like in the `range_do` macro.
    if (i > stop) return;
    loop {
        $f(i);
        if (i >= stop) break;
        i = i + 1;
    }
}

public macro fun do<$T, $R: drop>($stop: $T, $f: |$T| -> $R) {
    range_do!(0, $stop, $f)
}

public macro fun do_eq<$T, $R: drop>($stop: $T, $f: |$T| -> $R) {
    range_do_eq!(0, $stop, $f)
}

public(package) macro fun try_as_u8($x: _): Option<u8> {
    let x = $x;
    if (x > 0xFF) option::none() else option::some(x as u8)
}

public(package) macro fun try_as_u16($x: _): Option<u16> {
    let x = $x;
    if (x > 0xFFFF) option::none() else option::some(x as u16)
}

public(package) macro fun try_as_u32($x: _): Option<u32> {
    let x = $x;
    if (x > 0xFFFF_FFFF) option::none() else option::some(x as u32)
}

public(package) macro fun try_as_u64($x: _): Option<u64> {
    let x = $x;
    if (x > 0xFFFF_FFFF_FFFF_FFFF) option::none() else option::some(x as u64)
}

public(package) macro fun try_as_u128($x: _): Option<u128> {
    let x = $x;
    if (x > 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF) option::none() else option::some(x as u128)
}

/// Creates a fixed-point value from a quotient specified by its numerator and denominator.
/// `$T` is the underlying integer type for the fixed-point value, where `$T` has `$t_bits` bits.
/// `$U` is the type used for intermediate calculations, where `$U` is the next larger integer type.
/// `$max_t` is the maximum value that can be represented by `$T`.
/// `$t_bits` (as mentioned above) is the total number of bits in the fixed-point value (integer
/// plus fractional).
/// `$fractional_bits` is the number of fractional bits in the fixed-point value.
public(package) macro fun uq_from_quotient<$T, $U>(
    $numerator: $T,
    $denominator: $T,
    $max_t: $T,
    $t_bits: u8,
    $fractional_bits: u8,
    $abort_denominator: _,
    $abort_quotient_too_small: _,
    $abort_quotient_too_large: _,
): $T {
    let numerator = $numerator;
    let denominator = $denominator;
    if (denominator == 0) $abort_denominator;

    // Scale the numerator to have `$t_bits` fractional bits and the denominator to have
    // `$t_bits - $fractional_bits` fractional bits, so that the quotient will have
    // `$fractional_bits` fractional bits.
    let scaled_numerator = numerator as $U << $t_bits;
    let scaled_denominator = denominator as $U << ($t_bits - $fractional_bits);
    let quotient = scaled_numerator / scaled_denominator;

    // The quotient can only be zero if the numerator is also zero.
    if (quotient == 0 && numerator != 0) $abort_quotient_too_small;

    // Return the quotient as a fixed-point number. We first need to check whether the cast
    // can succeed.
    if (quotient > $max_t as $U) $abort_quotient_too_large;
    quotient as $T
}

public(package) macro fun uq_from_int<$T, $U>($integer: $T, $fractional_bits: u8): $U {
    ($integer as $U) << $fractional_bits
}

public(package) macro fun uq_add<$T, $U>($a: $T, $b: $T, $max_t: $T, $abort_overflow: _): $T {
    let sum = $a as $U + ($b as $U);
    if (sum > $max_t as $U) $abort_overflow;
    sum as $T
}

public(package) macro fun uq_sub<$T>($a: $T, $b: $T, $abort_overflow: _): $T {
    let a = $a;
    let b = $b;
    if (a < b) $abort_overflow;
    a - b
}

public(package) macro fun uq_to_int<$T, $U>($a: $U, $fractional_bits: u8): $T {
    ($a >> $fractional_bits) as $T
}

public(package) macro fun uq_int_mul<$T, $U>(
    $val: $T,
    $multiplier: $T,
    $max_t: $T,
    $fractional_bits: u8,
    $abort_overflow: _,
): $T {
    // The product of two `$T` bit values has the same number of bits as `$U`, so perform the
    // multiplication with `$U` types and keep the full `$U` bit product
    // to avoid losing accuracy.
    let unscaled_product = $val as $U * ($multiplier as $U);
    // The unscaled product has `$fractional_bits` fractional bits (from the multiplier)
    // so rescale it by shifting away the low bits.
    let product = unscaled_product >> $fractional_bits;
    // Check whether the value is too large.
    if (product > $max_t as $U) $abort_overflow;
    product as $T
}

public(package) macro fun uq_int_div<$T, $U>(
    $val: $T,
    $divisor: $T,
    $max_t: $T,
    $fractional_bits: u8,
    $abort_division_by_zero: _,
    $abort_overflow: _,
): $T {
    let val = $val;
    let divisor = $divisor;
    // Check for division by zero.
    if (divisor == 0) $abort_division_by_zero;
    // First convert to $U to increase the number of bits to the next integer size
    // and then shift left to add `$fractional_bits` fractional zero bits to the dividend.
    let scaled_value = val as $U << $fractional_bits;
    let quotient = scaled_value / (divisor as $U);
    // Check whether the value is too large.
    if (quotient > $max_t as $U) $abort_overflow;
    quotient as $T
}
