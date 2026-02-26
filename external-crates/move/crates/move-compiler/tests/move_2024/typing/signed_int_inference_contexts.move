// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Untyped literal inferred as signed from context
    fun infer_from_annotation_i8() {
        let _x: i8 = 1;
    }

    fun infer_from_annotation_i16() {
        let _x: i16 = 100;
    }

    fun infer_from_annotation_i32() {
        let _x: i32 = 1000;
    }

    fun infer_from_annotation_i64() {
        let _x: i64 = 1000000;
    }

    fun infer_from_annotation_i128() {
        let _x: i128 = 1000000000000;
    }

    // Inferred from function return type
    fun returns_i64(): i64 {
        42
    }

    // Inferred from function parameter
    fun takes_i32(x: i32): i32 {
        x
    }

    fun call_takes_i32() {
        let _x = takes_i32(10);
    }

    // Negation forces signed inference
    fun neg_infers_signed() {
        let _x: i64 = -1;
    }

    // Negation with explicit signed type
    fun neg_with_typed_literal() {
        let _x = -1i64;
    }

    // Negation with annotation
    fun neg_with_annotation() {
        let _x: i32 = -5;
    }

    // Binary op with one signed operand infers the other
    fun infer_from_binary_op() {
        let _x = 1i64 + 2;
    }

    fun infer_from_binary_op_reverse() {
        let _x = 2 + 1i64;
    }

    // Comparison with one signed operand
    fun infer_comparison() {
        let _x = 1i32 < 2;
    }
}
