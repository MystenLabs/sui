// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Invalid negative literals in match-pattern position.
module 0x42::m {
    // Negative pattern literals must carry a signed suffix: unlike expressions, patterns have no
    // negation operation whose type could be inferred later.
    fun untyped_negative(x: i8): u64 {
        match (x) {
            -1 => 0,
            _ => 1,
        }
    }

    // Unsigned literals cannot be negated
    fun negative_unsigned(x: u8): u64 {
        match (x) {
            -1u8 => 0,
            _ => 1,
        }
    }

    // Out of range: one past MIN
    fun negative_overflow(x: i8): u64 {
        match (x) {
            -129i8 => 0,
            _ => 1,
        }
    }

    fun negative_overflow_i256(x: i256): u64 {
        match (x) {
            -57896044618658097711785492504343953926634992332820282019728792003956564819969i256 =>
                0,
            _ => 1,
        }
    }
}
