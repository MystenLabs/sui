// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that overflow is detected and fix suggestions preserve the signed/unsigned
// distinction when types flow through variable unification.

module 0x42::m {

    // === Signed overflow through variable unification ===
    // Overflow detected even when type comes through a variable, not a direct annotation.

    fun signed_overflow_through_param_i8() {
        let _x: i8 = 128;
    }

    fun signed_overflow_through_binop_i16() {
        let x: i16 = 0;
        let _y = x + 32768;
    }

    fun signed_overflow_through_return_i32(): i32 {
        2147483648
    }

    fun signed_overflow_through_multi_hop_i64() {
        let a: i64 = 0;
        let _b = a + 9223372036854775808;
    }

    // === Unsigned overflow through variable unification ===
    // Ensures unsigned overflow suggestions stay unsigned.

    fun unsigned_overflow_through_binop_u8() {
        let x: u8 = 0;
        let _y = x + 256;
    }

    fun unsigned_overflow_through_binop_u16() {
        let x: u16 = 0;
        let _y = x + 65536;
    }
}
