// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests constant folding with negative values, exercising:
// - Arithmetic right shift (must sign-extend, not zero-fill)
// - Sign-extending widening casts
// - Signed range checking for narrowing casts
module 0x42::m {

    // Arithmetic right shift of negative values
    fun shr_neg_i8() {
        let _x: i8 = -16i8 >> 2u8; // -16 >> 2 = -4 (arithmetic shift)
    }

    fun shr_neg_i64() {
        let _x: i64 = -1024i64 >> 3u8; // -1024 >> 3 = -128
    }

    fun shr_neg_one() {
        let _x: i8 = -1i8 >> 1u8; // -1 >> 1 = -1 (all bits set stays all bits set)
    }

    // Widening casts: sign extension
    fun widen_neg_i8_to_i16() {
        let _x: i16 = (-1i8 as i16); // -1i8 should become -1i16, not 255i16
    }

    fun widen_neg_i8_to_i64() {
        let _x: i64 = (-1i8 as i64); // -1i8 should become -1i64
    }

    fun widen_neg_i16_to_i32() {
        let _x: i32 = (-100i16 as i32); // -100i16 should become -100i32
    }

    fun widen_neg_i32_to_i128() {
        let _x: i128 = (-1i32 as i128);
    }

    // Narrowing casts: signed range check
    fun narrow_to_i8() {
        let _x: i8 = (42i64 as i8); // 42 fits in i8
    }

    fun narrow_neg_to_i8() {
        let _x: i8 = (-42i64 as i8); // -42 fits in i8
    }

    fun narrow_i16_to_i8() {
        let _x: i8 = (-100i16 as i8); // -100 fits in i8
    }

    // Arithmetic with negative results
    fun sub_to_negative() {
        let _x: i64 = 5i64 - 10i64; // = -5
    }

    fun mul_by_negative() {
        let _x: i32 = 7i32 * -3i32; // = -21
    }

    // Comparison with negative values
    fun cmp_neg() {
        let _a: bool = -1i64 < 0i64; // true
        let _b: bool = -1i64 < 1i64; // true
        let _c: bool = -5i8 > -10i8; // true
    }
}
