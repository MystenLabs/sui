// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that signed integer types and literals are rejected in legacy edition.
module a::m {
    fun type_annotations() {
        let _a: i8 = 0;
        let _b: i16 = 0;
        let _c: i32 = 0;
        let _d: i64 = 0;
        let _e: i128 = 0;
        let _f: i256 = 0;
    }

    fun literal_suffixes() {
        let _a = 1i8;
        let _b = 1i16;
        let _c = 1i32;
        let _d = 1i64;
        let _e = 1i128;
        let _f = 1i256;
    }
}
