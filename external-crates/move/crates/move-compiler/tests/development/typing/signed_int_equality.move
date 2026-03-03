// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Equality between same signed types
    fun eq_i8() {
        let _x = (1i8 == 1i8);
    }

    fun eq_i16() {
        let _x = (1i16 == 2i16);
    }

    fun eq_i32() {
        let _x = (1i32 == 1i32);
    }

    fun eq_i64() {
        let _x = (1i64 == 1i64);
    }

    fun eq_i128() {
        let _x = (1i128 == 1i128);
    }

    // Inequality
    fun neq_i64() {
        let _x = (1i64 != 2i64);
    }

    // Equality with negated values
    fun eq_neg() {
        let a: i64 = 5i64;
        let _x = (-a == -5i64);
    }

    // Equality in if condition
    fun eq_in_if() {
        let x: i32 = 1i32;
        if (x == 1i32) { };
    }

    // Not equal to zero
    fun neq_zero() {
        let x: i64 = 42i64;
        let _b = (x != 0i64);
    }
}
