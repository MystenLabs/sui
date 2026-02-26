// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Untyped literal that exceeds signed range when inferred
    fun i8_overflow_inferred() {
        let _x: i8 = 128;
    }

    fun i8_ok_inferred() {
        let _x: i8 = 127;
    }

    fun i16_overflow_inferred() {
        let _x: i16 = 32768;
    }

    fun i16_ok_inferred() {
        let _x: i16 = 32767;
    }

    fun i32_overflow_inferred() {
        let _x: i32 = 2147483648;
    }

    fun i32_ok_inferred() {
        let _x: i32 = 2147483647;
    }

    fun i64_overflow_inferred() {
        let _x: i64 = 9223372036854775808;
    }

    fun i64_ok_inferred() {
        let _x: i64 = 9223372036854775807;
    }
}
