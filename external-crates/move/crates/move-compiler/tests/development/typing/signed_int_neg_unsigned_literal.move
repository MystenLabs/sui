// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Negating unsigned typed literals should error
    fun neg_u8() {
        let _x = -1u8;
    }

    fun neg_u16() {
        let _x = -1u16;
    }

    fun neg_u32() {
        let _x = -1u32;
    }

    fun neg_u64() {
        let _x = -1u64;
    }

    fun neg_u128() {
        let _x = -1u128;
    }

    fun neg_u256() {
        let _x = -1u256;
    }

    // Negating unsigned variable should error
    fun neg_unsigned_var() {
        let x: u64 = 5;
        let _y = -x;
    }
}
