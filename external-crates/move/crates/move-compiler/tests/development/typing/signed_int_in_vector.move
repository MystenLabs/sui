// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    fun vector_i8() {
        let _v = vector[1i8, 2i8, 3i8];
    }

    fun vector_i16() {
        let _v = vector[1i16, 2i16, 3i16];
    }

    fun vector_i32() {
        let _v = vector[1i32, 2i32, 3i32];
    }

    fun vector_i64() {
        let _v = vector[1i64, 2i64, 3i64];
    }

    fun vector_i128() {
        let _v = vector[1i128, 2i128, 3i128];
    }

    // Vector with negated elements
    fun vector_neg() {
        let x = 5i64;
        let _v = vector[-x, 1i64, -x];
    }

    // Empty vector with signed annotation
    fun vector_empty_i8() {
        let _v: vector<i8> = vector[];
    }

    // Vector with inferred signed type
    fun vector_inferred() {
        let _v: vector<i32> = vector[1, 2, 3];
    }
}
