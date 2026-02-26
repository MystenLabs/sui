// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Hex literals with signed suffix
    fun hex_i8() {
        let _x: i8 = 0x01i8;
    }

    fun hex_i8_max() {
        let _x: i8 = 0x7Fi8;
    }

    fun hex_i16() {
        let _x: i16 = 0x00FFi16;
    }

    fun hex_i32() {
        let _x: i32 = 0x0000FFFFi32;
    }

    fun hex_i64() {
        let _x: i64 = 0x0Fi64;
    }

    fun hex_i128() {
        let _x: i128 = 0xABCDi128;
    }

    // Zero in hex
    fun hex_zero() {
        let _x: i64 = 0x0i64;
    }
}
