// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: boolean match with only one arm.
// Targets: boolean literal exhaustiveness paths, lines 278/504/507.
module 0x0::M {
    fun f(b: bool): u64 {
        match (b) {
            true => 1,
        }
    }
}
