// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Casting between signed types of different widths
    fun i8_to_i16() {
        let x: i8 = 1i8;
        let _y = (x as i16);
    }

    fun i8_to_i32() {
        let x: i8 = 1i8;
        let _y = (x as i32);
    }

    fun i8_to_i64() {
        let x: i8 = 1i8;
        let _y = (x as i64);
    }

    fun i8_to_i128() {
        let x: i8 = 1i8;
        let _y = (x as i128);
    }

    fun i128_to_i64() {
        let x: i128 = 1i128;
        let _y = (x as i64);
    }

    fun i128_to_i8() {
        let x: i128 = 1i128;
        let _y = (x as i8);
    }

    fun i64_to_i32() {
        let x: i64 = 1i64;
        let _y = (x as i32);
    }

    fun i32_to_i16() {
        let x: i32 = 1i32;
        let _y = (x as i16);
    }

    fun i16_to_i8() {
        let x: i16 = 1i16;
        let _y = (x as i8);
    }

    // Same-type cast (no-op)
    fun i64_to_i64() {
        let x: i64 = 1i64;
        let _y = (x as i64);
    }
}
