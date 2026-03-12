// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that unification across variables correctly preserves the signed/unsigned
// distinction for the is_signed_numeric check in inferred_numerical_value.

module 0x42::m {

    // === Signed inference through variable unification ===

    // Type flows from annotated variable to literal through assignment.
    fun signed_through_reassignment_i8() {
        let x: i8 = 0;
        let _y = x;
        let _z: i8 = 100;
    }

    // Type flows from function parameter to literal via variable.
    fun takes_i16(x: i16): i16 { x }
    fun signed_through_param_i16() {
        let x = takes_i16(5);
        let _y = x;
    }

    // Type flows from return position through variable binding.
    fun returns_i32(): i32 { 42 }
    fun signed_through_return_i32() {
        let x = returns_i32();
        let _y = x;
    }

    // Binary op forces unification: typed operand propagates to untyped literal.
    fun signed_through_binop_i64() {
        let x: i64 = 10;
        let _y = x + 5;
    }

    fun signed_through_binop_reverse_i64() {
        let x: i64 = 10;
        let _y = 5 + x;
    }

    // Comparison forces unification across variables.
    fun signed_through_comparison_i32() {
        let x: i32 = 10;
        let _y = x < 20;
    }

    // Multiple hops: literal -> var -> var -> use.
    fun signed_multi_hop_i128() {
        let a: i128 = 99;
        let b = a;
        let _c = b;
    }

    // === Unsigned inference through variable unification ===
    // Ensures signed path doesn't accidentally fire for unsigned types.

    fun unsigned_through_reassignment_u8() {
        let x: u8 = 0;
        let _y = x;
        let _z: u8 = 200;
    }

    fun unsigned_through_binop_u64() {
        let x: u64 = 10;
        let _y = x + 18446744073709551000;
    }
}
