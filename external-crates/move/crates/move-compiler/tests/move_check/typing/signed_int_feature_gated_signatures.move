// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that signed integer types are rejected in struct fields, function
// parameters, return types, and constants in legacy edition.
module a::m {
    struct S has drop {
        x: i32,
        y: i64,
    }

    fun params(_a: i8, _b: i128): i64 {
        0
    }

    const MY_CONST: i32 = 0;
}
