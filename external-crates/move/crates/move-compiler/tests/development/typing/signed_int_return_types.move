// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Return signed from function
    fun ret_i8(): i8 { 1i8 }
    fun ret_i16(): i16 { 1i16 }
    fun ret_i32(): i32 { 1i32 }
    fun ret_i64(): i64 { 1i64 }
    fun ret_i128(): i128 { 1i128 }

    // Return negated value
    fun ret_neg(): i64 {
        let x = 5i64;
        -x
    }

    // Return from if expression
    fun ret_conditional(b: bool): i32 {
        if (b) 1i32 else -1i32
    }

    // Multiple return with signed
    fun ret_tuple(): (i32, i64) {
        (1i32, 2i64)
    }

    // Return wrong signed type should error
    fun ret_wrong_signed(): i64 {
        1i32
    }

    // Return unsigned when signed expected should error
    fun ret_unsigned_for_signed(): i64 {
        1u64
    }

    // Return signed when unsigned expected should error
    fun ret_signed_for_unsigned(): u64 {
        1i64
    }
}
