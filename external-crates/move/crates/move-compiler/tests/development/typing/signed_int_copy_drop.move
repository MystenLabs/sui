// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Signed ints have copy and drop (primitives)
    fun copy_signed() {
        let x: i64 = 42i64;
        let _a = copy x;
        let _b = x;
    }

    fun copy_i8() {
        let x: i8 = 1i8;
        let _a = copy x;
        let _b = copy x;
        let _c = x;
    }

    // Signed ints in struct with copy + drop
    struct S has copy, drop {
        a: i8,
        b: i16,
        c: i32,
        d: i64,
        e: i128,
    }

    fun copy_struct_with_signed() {
        let s = S { a: 1i8, b: 2i16, c: 3i32, d: 4i64, e: 5i128 };
        let _s2 = copy s;
        let _s3 = s;
    }

    // Signed int as type parameter
    fun vector_copy() {
        let v = vector[1i64, 2i64];
        let _v2 = copy v;
        let _v3 = v;
    }
}
