// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::uq64_64_tests;

use std::unit_test::assert_eq;
use std::uq64_64::{
    Self,
    add,
    sub,
    mul,
    div,
    int_div,
    int_mul,
    from_int,
    from_quotient,
    from_raw,
    to_raw,
};

#[test]
fun from_quotient_zero() {
    let x = from_quotient(0, 1);
    assert_eq!(x.to_raw(), 0);
}

#[test]
fun from_quotient_max_numerator_denominator() {
    // Test creating a 1.0 fraction from the maximum u128 value.
    let f = from_quotient(std::u128::max_value!(), std::u128::max_value!());
    let one = f.to_raw();
    assert_eq!(one, 1 << 64); // 0x1.00000000
}

#[test]
#[expected_failure(abort_code = uq64_64::EDenominator)]
fun from_quotient_div_zero() {
    // A denominator of zero should cause an arithmetic error.
    from_quotient(2, 0);
}

#[test]
#[expected_failure(abort_code = uq64_64::EQuotientTooLarge)]
fun from_quotient_ratio_too_large() {
    // The maximum value is 2^64 - 1. Check that anything larger aborts
    // with an overflow.
    from_quotient(1 << 64, 1); // 2^64
}

#[test]
#[expected_failure(abort_code = uq64_64::EQuotientTooSmall)]
fun from_quotient_ratio_too_small() {
    // The minimum non-zero value is 2^-64. Check that anything smaller
    // aborts.
    from_quotient(1, (1 << 64) + 1); // 1/(2^64 + 1)
}

#[test]
fun test_from_int() {
    assert_eq!(from_int(0).to_raw(), 0);
    assert_eq!(from_int(1).to_raw(), 0x1_0000_0000_0000_0000);
    assert_eq!(from_int(std::u64::max_value!()).to_raw(), std::u64::max_value!() as u128 << 64);
}

#[test]
fun test_add() {
    let a = from_quotient(3, 4);
    assert!(a.add(from_int(0)) == a);

    let c = a.add(from_int(1));
    assert!(from_quotient(7, 4) == c);

    let b = from_quotient(1, 4);
    let c = a.add(b);
    assert!(from_int(1) == c);
}

#[test]
#[expected_failure(abort_code = uq64_64::EOverflow)]
fun test_add_overflow() {
    let a = from_int(1 << 63);
    let b = from_int(1 << 63);
    let _ = a.add(b);
}

#[test]
fun test_sub() {
    let a = from_int(5);
    assert_eq!(a.sub(from_int(0)), a);

    let b = from_int(4);
    let c = a.sub(b);
    assert_eq!(from_int(1), c);
}

#[test]
#[expected_failure(abort_code = uq64_64::EOverflow)]
fun test_sub_underflow() {
    let a = from_int(3);
    let b = from_int(5);
    a.sub(b);
}

#[test]
fun test_mul() {
    let a = from_quotient(3, 4);
    assert!(a.mul(from_int(0)) == from_int(0));
    assert!(a.mul(from_int(1)) == a);

    let b = from_quotient(3, 2);
    let c = a.mul(b);
    let expected = from_quotient(9, 8);
    assert_eq!(c, expected);
}

#[test]
#[expected_failure(abort_code = uq64_64::EOverflow)]
fun test_mul_overflow() {
    let a = from_int(1 << 32);
    let b = from_int(1 << 32);
    let _ = a.mul(b);
}

#[test]
fun test_div() {
    let a = from_quotient(3, 4);
    assert!(a.div(from_int(1)) == a);

    let b = from_int(8);
    let c = a.div(b);
    let expected = from_quotient(3, 32);
    assert_eq!(c, expected);
}

#[test]
#[expected_failure(abort_code = uq64_64::EDivisionByZero)]
fun test_div_by_zero() {
    let a = from_int(7);
    let b = from_int(0);
    let _ = a.div(b);
}

#[test]
#[expected_failure(abort_code = uq64_64::EOverflow)]
fun test_div_overflow() {
    let a = from_int(1 << 63);
    let b = from_quotient(1, 2);
    let _ = a.div(b);
}

#[test]
fun exact_int_div() {
    let f = from_quotient(3, 4); // 0.75
    let twelve = int_div(9, f); // 9 / 0.75
    assert_eq!(twelve, 12);
}

#[test]
#[expected_failure(abort_code = uq64_64::EDivisionByZero)]
fun int_div_by_zero() {
    let f = from_raw(0); // 0
    // Dividing by zero should cause an arithmetic error.
    int_div(1, f);
}

#[test]
#[expected_failure(abort_code = uq64_64::EOverflow)]
fun int_div_overflow_small_divisor() {
    let f = from_raw(1); // 0x0.00000001
    // Divide 2^64 by the minimum fractional value. This should overflow.
    int_div(1 << 64, f);
}

#[test]
#[expected_failure(abort_code = uq64_64::EOverflow)]
fun int_div_overflow_large_numerator() {
    let f = from_quotient(1, 2); // 0.5
    // Divide the maximum u128 value by 0.5. This should overflow.
    int_div(std::u128::max_value!(), f);
}

#[test]
fun exact_int_mul() {
    let f = from_quotient(3, 4); // 0.75
    let nine = int_mul(12, f); // 12 * 0.75
    assert_eq!(nine, 9);
}

#[test]
fun int_mul_truncates() {
    let f = from_quotient(1, 3); // 0.333...
    let not_three = int_mul(9, copy f); // 9 * 0.333...
    // multiply_u128 does NOT round -- it truncates -- so values that
    // are not perfectly representable in binary may be off by one.
    assert_eq!(not_three, 2);

    // Try again with a fraction slightly larger than 1/3.
    let f = from_raw(f.to_raw() + 1);
    let three = int_mul(9, f);
    assert_eq!(three, 3);
}

#[test]
#[expected_failure(abort_code = uq64_64::EOverflow)]
fun int_mul_overflow_small_multiplier() {
    let f = from_quotient(3, 2); // 1.5
    // Multiply the maximum u128 value by 1.5. This should overflow.
    int_mul(std::u128::max_value!(), f);
}

#[test]
#[expected_failure(abort_code = uq64_64::EOverflow)]
fun int_mul_overflow_large_multiplier() {
    let f = from_raw(std::u128::max_value!());
    // Multiply 2^64 + 1 by the maximum fixed-point value. This should overflow.
    int_mul((1 << 64) + 1, f);
}

#[test]
fun test_comparison() {
    let a = from_quotient(5, 2);
    let b = from_quotient(5, 3);
    let c = from_quotient(5, 2);

    assert!(b.le(a));
    assert!(b.lt(a));
    assert!(c.le(a));
    assert_eq!(c, a);
    assert!(a.ge(b));
    assert!(a.gt(b));
    assert!(from_int(0).le(a));
}

#[random_test]
fun test_raw(raw: u128) {
    assert_eq!(from_raw(raw).to_raw(), raw);
}

#[random_test]
fun test_int_roundtrip(c: u64) {
    assert_eq!(from_int(c).to_int(), c);
}

#[random_test]
fun test_mul_rand(n: u32, d: u32, c: u32) {
    if (d == 0) return;
    let q = from_quotient(n as u128, d as u128);
    assert_eq!(int_mul(c as u128, q), q.mul(from_int(c as u64)).to_int() as u128);
}

#[random_test]
fun test_div_rand(n: u32, d: u32, c: u32) {
    if (d == 0) return;
    let q = from_quotient(n as u128, d as u128);
    assert_eq!(int_div(c as u128, q), from_int(c as u64).div(q).to_int() as u128);
}
