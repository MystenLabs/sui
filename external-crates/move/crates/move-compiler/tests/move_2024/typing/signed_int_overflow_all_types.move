// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Overflow: value exceeds non-negative range of signed type

    fun i8_overflow() {
        let _x: i8 = 128i8;
    }

    fun i8_overflow_large() {
        let _x: i8 = 255i8;
    }

    fun i16_overflow() {
        let _x: i16 = 32768i16;
    }

    fun i16_overflow_large() {
        let _x: i16 = 65535i16;
    }

    fun i32_overflow() {
        let _x: i32 = 2147483648i32;
    }

    fun i64_overflow() {
        let _x: i64 = 9223372036854775808i64;
    }

    fun i128_overflow() {
        let _x: i128 = 170141183460469231731687303715884105728i128;
    }

    // Hex literal overflow
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
}
