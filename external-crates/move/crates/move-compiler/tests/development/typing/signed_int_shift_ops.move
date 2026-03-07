// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Shift operations on signed integers
    fun shl_i8() {
        let _x = 1i8 << 2u8;
    }

    fun shr_i8() {
        let _x = 64i8 >> 2u8;
    }

    fun shl_i16() {
        let _x = 1i16 << 4u8;
    }

    fun shr_i16() {
        let _x = 256i16 >> 4u8;
    }

    fun shl_i32() {
        let _x = 1i32 << 8u8;
    }

    fun shr_i32() {
        let _x = 65536i32 >> 8u8;
    }

    fun shl_i64() {
        let _x = 1i64 << 16u8;
    }

    fun shr_i64() {
        let _x = 65536i64 >> 16u8;
    }

    fun shl_i128() {
        let _x = 1i128 << 32u8;
    }

    fun shr_i128() {
        let _x = 4294967296i128 >> 32u8;
    }
}
