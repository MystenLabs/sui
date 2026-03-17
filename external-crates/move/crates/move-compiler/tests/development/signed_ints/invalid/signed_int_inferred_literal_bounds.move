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

    fun i256_overflow_inferred() {
        let _x: i256 = 57896044618658097711785492504343953926634992332820282019728792003956564819968;
    }

    fun i256_ok_inferred() {
        let _x: i256 = 57896044618658097711785492504343953926634992332820282019728792003956564819967;
    }

    // u256::MAX does not fit in i256
    fun u256_max_does_not_fit_i256() {
        let _x: i256 = 115792089237316195423570985008687907853269984665640564039457584007913129639935;
    }

    // Negated u256::MAX does not fit in i256 either
    fun neg_u256_max_does_not_fit_i256() {
        let _x: i256 = -115792089237316195423570985008687907853269984665640564039457584007913129639935i256;
    }

    // 2^255 (i256::MAX + 1) does not fit as a positive i256...
    fun i256_max_plus_one_does_not_fit() {
        let _x: i256 = 57896044618658097711785492504343953926634992332820282019728792003956564819968i256;
    }

    // ...but negated it is exactly i256::MIN and fits.
    // See signed_int_min_values.move for the valid counterpart.
}
