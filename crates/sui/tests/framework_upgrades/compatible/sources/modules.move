// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Test modules to be added to the sui framework in msim builds, to
 * test framework upgrades.  The module contents below represents the
 * framework *after* it a compatible upgrade
 */

module sui::msim_extra_1 {
    struct Type has drop, store {
        x: u64
    }

    struct NewType {
        t: Type,
    }

    public fun canary(): u64 {
        private_function(20, 21)
    }

    fun private_function(x: u64, y: u64): u64 {
        x + y + 2
    }

    public fun generic<T: drop>(_t: T) {}
}

module sui::msim_extra_2 {
    public fun bar(): u64 {
        43
    }
}
