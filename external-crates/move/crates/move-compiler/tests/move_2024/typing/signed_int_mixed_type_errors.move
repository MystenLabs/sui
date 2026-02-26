// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Mixing signed and unsigned in binary ops should error
    fun add_signed_unsigned() {
        let _x = 1i8 + 1u8;
    }

    fun sub_signed_unsigned() {
        let _x = 1i16 - 1u16;
    }

    fun mul_signed_unsigned() {
        let _x = 1i32 * 1u32;
    }

    fun div_signed_unsigned() {
        let _x = 1i64 / 1u64;
    }

    fun mod_signed_unsigned() {
        let _x = 1i128 % 1u128;
    }

    // Mixing different signed types should error
    fun add_different_signed() {
        let _x = 1i8 + 1i16;
    }

    fun sub_different_signed() {
        let _x = 1i32 - 1i64;
    }

    // Comparison between signed and unsigned should error
    fun cmp_signed_unsigned() {
        let _x = 1i64 < 1u64;
    }

    fun eq_signed_unsigned() {
        let _x = 1i32 == 1u32;
    }

    fun neq_signed_unsigned() {
        let _x = 1i8 != 1u8;
    }

    // Bitwise between signed and unsigned should error
    fun bitand_signed_unsigned() {
        let _x = 1i64 & 1u64;
    }

    fun bitor_signed_unsigned() {
        let _x = 1i32 | 1u32;
    }

    fun bitxor_signed_unsigned() {
        let _x = 1i16 ^ 1u16;
    }
}
