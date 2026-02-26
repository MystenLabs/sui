// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // i8 boundaries: 0 to 127 (non-negative range, since negation is separate)
    fun i8_zero() {
        let _x: i8 = 0i8;
    }

    fun i8_max() {
        let _x: i8 = 127i8;
    }

    // i16 boundaries: 0 to 32767
    fun i16_zero() {
        let _x: i16 = 0i16;
    }

    fun i16_max() {
        let _x: i16 = 32767i16;
    }

    // i32 boundaries: 0 to 2147483647
    fun i32_zero() {
        let _x: i32 = 0i32;
    }

    fun i32_max() {
        let _x: i32 = 2147483647i32;
    }

    // i64 boundaries: 0 to 9223372036854775807
    fun i64_zero() {
        let _x: i64 = 0i64;
    }

    fun i64_max() {
        let _x: i64 = 9223372036854775807i64;
    }

    // i128 boundaries: 0 to 170141183460469231731687303715884105727
    fun i128_zero() {
        let _x: i128 = 0i128;
    }

    fun i128_max() {
        let _x: i128 = 170141183460469231731687303715884105727i128;
    }

    // Negation of zero
    fun neg_zero_i8() {
        let _x: i8 = -(0i8);
    }

    fun neg_zero_i64() {
        let _x: i64 = -(0i64);
    }

    // Negation of max (produces min + 1)
    fun neg_max_i8() {
        let _x: i8 = -(127i8);
    }

    fun neg_max_i64() {
        let _x: i64 = -(9223372036854775807i64);
    }
}
