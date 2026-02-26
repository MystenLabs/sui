// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Complex arithmetic chains
    fun chain_add() {
        let _x = 1i64 + 2i64 + 3i64 + 4i64 + 5i64;
    }

    fun chain_mixed_ops() {
        let _x = 10i32 + 20i32 - 5i32 * 2i32;
    }

    // Nested casts between signed types
    fun nested_cast() {
        let x: i8 = 1i8;
        let _y = ((x as i32) as i64);
    }

    // Conditional expression with signed
    fun ternary_like(b: bool): i64 {
        if (b) 1i64 else -1i64
    }

    // Comparison chains
    fun chained_comparison() {
        let a: i64 = 1i64;
        let b: i64 = 2i64;
        let c: i64 = 3i64;
        let _x = (a < b) && (b < c);
    }

    // Signed int in block expression
    fun block_expr(): i64 {
        let result = {
            let a = 10i64;
            let b = 20i64;
            a + b
        };
        result
    }

    // Negation of function call result
    fun get_val(): i64 { 42i64 }

    fun neg_call_result() {
        let _x = -(get_val());
    }

    // Arithmetic with negated subexpressions
    fun neg_in_arithmetic() {
        let a: i64 = 3i64;
        let b: i64 = 7i64;
        let _x = (-a) + (-b);
        let _y = (-a) * (-b);
        let _z = (-a) - (-b);
    }

    // Deeply nested negation
    fun deep_neg() {
        let x: i64 = 1i64;
        let _a = -(-(-(-x)));
    }
}
