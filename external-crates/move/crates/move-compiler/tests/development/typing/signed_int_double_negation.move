// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    fun double_neg_var() {
        let x: i64 = 5i64;
        let _a: i64 = -(-x);
    }

    fun triple_neg() {
        let x: i64 = 1i64;
        let _a: i64 = -(-(-x));
    }

    fun double_neg_literal() {
        let _a: i64 = -(-(3i64));
    }

    fun double_neg_i8() {
        let x: i8 = 1i8;
        let _a: i8 = -(-x);
    }

    fun double_neg_i128() {
        let x: i128 = 1i128;
        let _a: i128 = -(-x);
    }

    fun neg_parenthesized_expr() {
        let a: i32 = 10i32;
        let b: i32 = 3i32;
        let _c: i32 = -(a + b);
        let _d: i32 = -(a * b);
        let _e: i32 = -(a - b);
        let _f: i32 = -(a / b);
        let _g: i32 = -(a % b);
    }
}
