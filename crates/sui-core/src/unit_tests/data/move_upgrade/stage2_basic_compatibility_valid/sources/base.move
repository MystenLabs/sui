// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module base_addr::base {

    struct A<T> {
        f1: bool,
        f2: T
    }

    // Add a struct
    struct B<T> {
        f1: bool,
        f2: T
    }

    friend base_addr::friend_module;

    public fun return_0(): u64 { 0 }

    public fun plus_1(x: u64): u64 { x + 1 }

    // We currently cannot change a friend function as the loader will yell at us.
    public(friend) fun friend_fun(x: u64): u64 { x }

    // Change this private function
    fun non_public_fun(y: bool, g: u64): u64 { if (y) 0 else g }

    entry fun entry_fun() { }
}
