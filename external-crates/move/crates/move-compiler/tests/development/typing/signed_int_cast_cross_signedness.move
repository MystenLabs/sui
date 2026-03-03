// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Casting from unsigned to signed
    fun u8_to_i8() {
        let x: u8 = 1;
        let _y = (x as i8);
    }

    fun u16_to_i16() {
        let x: u16 = 1;
        let _y = (x as i16);
    }

    fun u32_to_i32() {
        let x: u32 = 1;
        let _y = (x as i32);
    }

    fun u64_to_i64() {
        let x: u64 = 1;
        let _y = (x as i64);
    }

    fun u128_to_i128() {
        let x: u128 = 1;
        let _y = (x as i128);
    }

    // Casting from signed to unsigned
    fun i8_to_u8() {
        let x: i8 = 1i8;
        let _y = (x as u8);
    }

    fun i16_to_u16() {
        let x: i16 = 1i16;
        let _y = (x as u16);
    }

    fun i32_to_u32() {
        let x: i32 = 1i32;
        let _y = (x as u32);
    }

    fun i64_to_u64() {
        let x: i64 = 1i64;
        let _y = (x as u64);
    }

    fun i128_to_u128() {
        let x: i128 = 1i128;
        let _y = (x as u128);
    }

    // Casting from signed to different-width unsigned
    fun i8_to_u64() {
        let x: i8 = 1i8;
        let _y = (x as u64);
    }

    fun i128_to_u8() {
        let x: i128 = 1i128;
        let _y = (x as u8);
    }

    // Casting from unsigned to different-width signed
    fun u8_to_i64() {
        let x: u8 = 1;
        let _y = (x as i64);
    }

    fun u256_to_i8() {
        let x: u256 = 1;
        let _y = (x as i8);
    }
}
