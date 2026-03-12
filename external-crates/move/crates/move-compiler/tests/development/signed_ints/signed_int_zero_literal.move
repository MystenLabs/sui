// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Zero is valid for all signed types
    fun zero_i8() { let _x = 0i8; }
    fun zero_i16() { let _x = 0i16; }
    fun zero_i32() { let _x = 0i32; }
    fun zero_i64() { let _x = 0i64; }
    fun zero_i128() { let _x = 0i128; }

    // Zero with negation
    fun neg_zero_i8() { let _x = -(0i8); }
    fun neg_zero_i64() { let _x = -(0i64); }

    // Inferred zero
    fun inferred_zero_i8() { let _x: i8 = 0; }
    fun inferred_zero_i64() { let _x: i64 = 0; }
    fun inferred_zero_i128() { let _x: i128 = 0; }

    // Arithmetic with zero
    fun add_zero() {
        let x = 5i64;
        let _y = x + 0i64;
    }

    fun sub_zero() {
        let x = 5i32;
        let _y = x - 0i32;
    }

    fun mul_zero() {
        let x = 5i16;
        let _y = x * 0i16;
    }
}
