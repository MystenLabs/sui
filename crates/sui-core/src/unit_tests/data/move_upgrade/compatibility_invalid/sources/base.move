// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module base_addr::base {

    public struct A<T> {
        f1: bool,
        f2: T
    }

    public fun return_0(): u64 { 0 }

    public fun plus_1(x: u64, y: u64): u64 { x + y }

    public(package) fun friend_fun(x: u64): u64 { x }

    fun non_public_fun(y: bool): u64 { if (y) 0 else 1 }

    entry fun entry_fun() { }
}
