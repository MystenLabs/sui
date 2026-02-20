// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x2::M {
    // return as match subject
    fun f_return(): u64 {
        match (return 0) {
            0u64 => 0u64,
            _ => 0u64,
        }
    }

    // loop as match subject
    fun f_loop(): u64 {
        match (loop {}) {
            0u64 => 0u64,
            _ => 0u64,
        }
    }

    // block ending in abort as match subject
    fun f_block_abort(): u64 {
        match ({ abort 0 }) {
            0u64 => 0u64,
            _ => 0u64,
        }
    }

    // block ending in return as match subject
    fun f_block_return(): u64 {
        match ({ return 0 }) {
            0u64 => 0u64,
            _ => 0u64,
        }
    }

    // nested divergence
    fun f_nested(): u64 {
        match (if (true) { abort 0 } else { return 1 }) {
            0u64 => 0u64,
            _ => 0u64,
        }
    }
}
