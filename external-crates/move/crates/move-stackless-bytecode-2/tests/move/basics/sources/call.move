// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module basics::call {
    fun call(a: u64, b: u64, c: u64): u64 { a + b + c }

    public fun do_it(): u64 { call(0, 1, 2) }
}
