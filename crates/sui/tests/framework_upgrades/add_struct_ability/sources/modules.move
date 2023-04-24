// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::msim_extra_1 {
    struct Type has drop, copy {
        x: u64,
    }

    public fun canary(): u64 {
        private_function(41)
    }

    entry fun entry_fun() {}

    fun private_function(x: u64): u64 {
        private_function_2(x) + 1
    }

    fun private_function_2(x: u64): u64 { x }
    fun private_function_3(_x: u64) {}

    public fun generic<T: copy + drop>(_t: T) {}
}
