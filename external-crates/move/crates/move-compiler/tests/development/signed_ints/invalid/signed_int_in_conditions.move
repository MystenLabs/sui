// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Signed int as if condition should error (not bool)
    fun if_signed() {
        if (1i64) { };
    }

    // Signed int as while condition should error
    fun while_signed() {
        while (1i32) { };
    }

    // Signed int in assert condition should error
    fun assert_signed() {
        assert!(1i8);
    }

    // These should be fine: comparison of signed ints produces bool
    fun if_signed_comparison() {
        let x: i64 = 5i64;
        if (x > 0i64) { };
    }

    fun while_signed_comparison() {
        let mut x: i32 = 10i32;
        while (x > 0i32) {
            x = x - 1i32;
        };
    }
}
