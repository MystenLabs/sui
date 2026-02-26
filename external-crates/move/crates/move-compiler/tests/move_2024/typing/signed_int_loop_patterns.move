// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Loop with signed counter
    fun countdown() {
        let mut i: i32 = 10i32;
        while (i > 0i32) {
            i = i - 1i32;
        };
    }

    // Signed int in loop with negation
    fun neg_in_loop() {
        let mut x: i64 = 100i64;
        while (x > 0i64) {
            x = x + (-10i64);
        };
    }

    // Break with signed value
    fun break_with_signed(): i64 {
        let mut i: i64 = 0i64;
        loop {
            if (i > 10i64) break i;
            i = i + 1i64;
        }
    }
}
