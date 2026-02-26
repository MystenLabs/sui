// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Negating bool should error
    fun neg_bool() {
        let _x = -true;
    }

    fun neg_bool_var() {
        let b = false;
        let _x = -b;
    }

    // Negating address should error
    fun neg_address() {
        let _x = -@0x42;
    }

    // Negating vector should error
    fun neg_vector() {
        let v = vector[1i8];
        let _x = -v;
    }

    // Negating struct should error
    struct S has copy, drop { val: i64 }

    fun neg_struct() {
        let s = S { val: 1i64 };
        let _x = -s;
    }
}
