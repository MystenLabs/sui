// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Overflow counterparts to signed_int_boundary_values: one past MAX, one past MIN,
// and one past the negatable range for each signed type.
module 0x42::m {
    // === Positive literal overflow (MAX + 1) ===

    fun i8_max_plus_one() {
        let _x: i8 = 128i8;
    }

    fun i16_max_plus_one() {
        let _x: i16 = 32768i16;
    }

    fun i32_max_plus_one() {
        let _x: i32 = 2147483648i32;
    }

    fun i64_max_plus_one() {
        let _x: i64 = 9223372036854775808i64;
    }

    fun i128_max_plus_one() {
        let _x: i128 = 170141183460469231731687303715884105728i128;
    }

    // === Negated literal overflow (one past MIN, i.e. -(MAX + 2)) ===

    fun i8_neg_min_minus_one() {
        let _x: i8 = -129i8;
    }

    fun i16_neg_min_minus_one() {
        let _x: i16 = -32769i16;
    }

    fun i32_neg_min_minus_one() {
        let _x: i32 = -2147483649i32;
    }

    fun i64_neg_min_minus_one() {
        let _x: i64 = -9223372036854775809i64;
    }

    fun i128_neg_min_minus_one() {
        let _x: i128 = -170141183460469231731687303715884105729i128;
    }

    // === Hex overflow (one past MAX) ===

    fun i8_hex_overflow() {
        let _x: i8 = 0x80i8;
    }

    fun i16_hex_overflow() {
        let _x: i16 = 0x8000i16;
    }

    fun i32_hex_overflow() {
        let _x: i32 = 0x80000000i32;
    }

    fun i64_hex_overflow() {
        let _x: i64 = 0x8000000000000000i64;
    }

    fun i128_hex_overflow() {
        let _x: i128 = 0x80000000000000000000000000000000i128;
    }
}
