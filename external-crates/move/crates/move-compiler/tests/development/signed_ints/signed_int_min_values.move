// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that MIN values for all signed types can be expressed via negation.
module 0x42::m {
    fun i8_min() {
        let _x: i8 = -128i8;
    }

    fun i16_min() {
        let _x: i16 = -32768i16;
    }

    fun i32_min() {
        let _x: i32 = -2147483648i32;
    }

    fun i64_min() {
        let _x: i64 = -9223372036854775808i64;
    }

    fun i128_min() {
        let _x: i128 = -170141183460469231731687303715884105728i128;
    }

    // MIN + 1 via negation (MAX negated)
    fun i8_neg_max() {
        let _x: i8 = -127i8;
    }

    fun i64_neg_max() {
        let _x: i64 = -9223372036854775807i64;
    }

    // Negation of 1
    fun i8_neg_one() {
        let _x: i8 = -1i8;
    }

    fun i64_neg_one() {
        let _x: i64 = -1i64;
    }

    // Negation of zero
    fun i8_neg_zero() {
        let _x: i8 = -0i8;
    }

    fun i64_neg_zero() {
        let _x: i64 = -0i64;
    }
}
