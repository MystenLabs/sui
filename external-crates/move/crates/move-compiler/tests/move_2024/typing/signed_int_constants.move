// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    const C_I8: i8 = 42i8;
    const C_I16: i16 = 1000i16;
    const C_I32: i32 = 100000i32;
    const C_I64: i64 = 1000000i64;
    const C_I128: i128 = 1000000000000i128;

    // Constant with zero
    const ZERO_I64: i64 = 0i64;

    // Constant with max values
    const MAX_I8: i8 = 127i8;
    const MAX_I16: i16 = 32767i16;
    const MAX_I32: i32 = 2147483647i32;
    const MAX_I64: i64 = 9223372036854775807i64;

    // Constant with negation
    const NEG_ONE: i64 = -1i64;
    const NEG_MAX_I8: i8 = -127i8;

    // Constant expression with arithmetic
    const EXPR: i64 = 10i64 + 20i64;
    const EXPR2: i32 = 100i32 - 50i32;
    const EXPR3: i16 = 6i16 * 7i16;

    // Use constants
    fun use_constants() {
        let _a = C_I8;
        let _b = C_I64;
        let _c = NEG_ONE;
        let _d = EXPR;
    }
}
