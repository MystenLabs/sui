// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Negation binds tighter than binary ops
    // -a + b  should parse as  (-a) + b
    fun neg_plus() {
        let a: i64 = 3i64;
        let b: i64 = 5i64;
        let _x: i64 = -a + b;
    }

    // -a * b  should parse as  (-a) * b
    fun neg_times() {
        let a: i64 = 3i64;
        let b: i64 = 5i64;
        let _x: i64 = -a * b;
    }

    // -a - b  should parse as  (-a) - b
    fun neg_minus() {
        let a: i64 = 3i64;
        let b: i64 = 5i64;
        let _x: i64 = -a - b;
    }

    // Explicit grouping: -(a + b)
    fun neg_grouped() {
        let a: i64 = 3i64;
        let b: i64 = 5i64;
        let _x: i64 = -(a + b);
    }

    // Mixed with logical not
    fun neg_and_not() {
        let a: i64 = 3i64;
        let _x: i64 = -a;
        let _y: bool = !true;
    }

    // Chained negation and arithmetic
    fun complex_expr() {
        let a: i32 = 10i32;
        let b: i32 = 3i32;
        let c: i32 = 7i32;
        let _x: i32 = -a + b * c;
        let _y: i32 = -(a + b) * c;
        let _z: i32 = -a + -b + -c;
    }
}
