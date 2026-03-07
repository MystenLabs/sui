// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that all signed integer types are gated behind the development edition.
module a::m {
    fun gated_i8() {
        let _x: i8 = 0;
    }

    fun gated_i16() {
        let _x: i16 = 0;
    }

    fun gated_i32() {
        let _x: i32 = 0;
    }

    fun gated_i64() {
        let _x: i64 = 0;
    }

    fun gated_i128() {
        let _x: i128 = 0;
    }

    fun gated_suffix_i8() {
        let _x = 1i8;
    }

    fun gated_suffix_i16() {
        let _x = 1i16;
    }

    fun gated_suffix_i32() {
        let _x = 1i32;
    }

    fun gated_suffix_i64() {
        let _x = 1i64;
    }

    fun gated_suffix_i128() {
        let _x = 1i128;
    }
}
