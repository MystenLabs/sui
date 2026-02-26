// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Assign signed to unsigned should error
    fun assign_signed_to_unsigned() {
        let _x: u64 = 1i64;
    }

    fun assign_unsigned_to_signed() {
        let _x: i64 = 1u64;
    }

    // Assign signed to wrong signed width should error
    fun assign_i8_to_i64() {
        let _x: i64 = 1i8;
    }

    fun assign_i64_to_i8() {
        let _x: i8 = 1i64;
    }

    // Reassignment with wrong type
    fun reassign_wrong_type() {
        let mut x: i32 = 1i32;
        x = 2u32;
    }

    // Reassignment same type (valid)
    fun reassign_same_type() {
        let mut x: i32 = 1i32;
        x = 2i32;
        let _y = x;
    }

    // Assign negated value
    fun assign_negated() {
        let mut x: i64 = 5i64;
        x = -x;
        let _y = x;
    }

    // Multiple assignment
    fun multiple_assign() {
        let (_a, _b): (i32, i64) = (1i32, 2i64);
    }

    // Signed in tuple with unsigned should be separate types
    fun tuple_mixed() {
        let (_a, _b): (i32, u32) = (1i32, 2u32);
    }
}
