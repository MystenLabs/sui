// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that negated literals with magnitude > abs(MIN) are rejected.
module 0x42::m {
    fun i8_neg_overflow() {
        let _x: i8 = -129i8;
    }

    fun i16_neg_overflow() {
        let _x: i16 = -32769i16;
    }

    fun i32_neg_overflow() {
        let _x: i32 = -2147483649i32;
    }

    fun i64_neg_overflow() {
        let _x: i64 = -9223372036854775809i64;
    }

    fun i128_neg_overflow() {
        let _x: i128 = -170141183460469231731687303715884105729i128;
    }

    fun i256_neg_overflow() {
        let _x: i256 = -57896044618658097711785492504343953926634992332820282019728792003956564819969i256;
    }
}
