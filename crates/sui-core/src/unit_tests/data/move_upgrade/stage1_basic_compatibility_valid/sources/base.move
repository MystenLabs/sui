// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module base_addr::base {

    public struct A<T> {
        f1: bool,
        f2: T
    }

    // Add a struct
    public struct B<T> {
        f1: bool,
        f2: T
    }

    public fun return_0(): u64 { 0 }

    public fun plus_1(x: u64): u64 { x + 1 }

    // We currently cannot change a friend function as the loader will yell at us.
    public(package) fun friend_fun(x: u64): u64 { x }

    // Change this private function
    fun non_public_fun(y: bool, g: u64): u64 { if (y) 0 else g }

    // Note that this is fine since the entry function is private
    entry fun entry_fun(x: u64): u64 { x }
}
