// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::Balance {
    // use Std::Errors; // TODO: error handling
    friend Sui::Coin;

    struct Balance<phantom T> has store {
        value: u64
    }

    public fun value<T>(self: &Balance<T>): u64 {
        self.value
    }

    public fun empty<T>(): Balance<T> {
        Balance { value: 0 }
    }



    public fun join<T>(self: &mut Balance<T>, balance: Balance<T>) {
        let Balance { value } = balance;
        self.value = self.value + value;
    }

    public fun split<T>(self: &mut Balance<T>, value: u64): Balance<T> {
        assert!(self.value >= value, 0);
        self.value = self.value - value;
        Balance { value }
    }

    public fun destroy_empty<T>(balance: Balance<T>) {
        assert!(balance.value == 0, 0);
        let Balance { value: _ } = balance;
    }

    /// Can only be called by Sui::Coin.
    public(friend) fun create<T>(value: u64): Balance<T> {
        Balance { value }
    }

    /// Can only be called by Sui::Coin.
    public(friend) fun destroy<T>(self: Balance<T>): u64 {
        let Balance { value } = self;
        value
    }
}
