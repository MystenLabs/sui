// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Test modules to be added to the sui framework in msim builds, to
 * test framework upgrades.  The module contents below represents the
 * framework *before* it has been upgraded.
 */

module sui::msim_extra_1 {
    struct Type has drop {
        x: u64
    }

    public fun canary(): u64 {
        private_function(41)
    }

    fun private_function(x: u64): u64 {
        x + 1
    }

    public fun generic<T: copy + drop>(_t: T) {}
}
