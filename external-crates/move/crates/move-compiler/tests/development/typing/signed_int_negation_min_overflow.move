// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests overflow behavior when negating MIN values.
// Negating MIN of a signed type overflows since abs(MIN) > MAX.
module 0x42::m {
    fun negate_min_i8() {
        let x: i8 = -128i8;
        let _y: i8 = -x; // overflows: -(-128) doesn't fit in i8
    }

    fun negate_min_i64() {
        let x: i64 = -9223372036854775808i64;
        let _y: i64 = -x; // overflows
    }

    // Double negation of a literal that isn't MIN
    fun double_neg_literal() {
        let _x: i8 = -(-127i8); // = 127, should be fine
    }

    // Narrowing cast overflow
    fun narrow_overflow() {
        let _x: i8 = (128i64 as i8); // 128 doesn't fit in i8
    }

    fun narrow_neg_overflow() {
        let _x: i8 = (-129i64 as i8); // -129 doesn't fit in i8
    }
}
