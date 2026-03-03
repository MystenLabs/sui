// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Generic function with signed int
    fun identity<T: copy + drop>(x: T): T { x }

    fun call_identity() {
        let _a = identity(1i8);
        let _b = identity(2i16);
        let _c = identity(3i32);
        let _d = identity(4i64);
        let _e = identity(5i128);
    }

    // Generic struct holding signed int
    struct Box<T> has copy, drop {
        val: T,
    }

    fun box_signed() {
        let _a = Box<i8> { val: 1i8 };
        let _b = Box<i64> { val: 42i64 };
        let _c = Box<i128> { val: 0i128 };
    }

    // Nested generic with signed
    fun nested_box() {
        let inner = Box<i32> { val: 10i32 };
        let _outer = Box<Box<i32>> { val: inner };
    }

    // Vector of signed in generic context
    fun vec_in_box() {
        let v = vector[1i64, 2i64, 3i64];
        let _b = Box<vector<i64>> { val: v };
    }
}
