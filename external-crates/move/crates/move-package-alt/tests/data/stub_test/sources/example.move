// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A small module that exercises various Move features for stub generation testing.
module 0xcafe::example {
    public struct Coin has copy, drop, store {
        value: u64,
    }

    public struct Box<phantom T: store> has store {
        id: u64,
        value: u64,
    }

    public struct Wrapper<T: store> has store {
        inner: T,
    }

    /// Struct with no abilities
    public struct Empty {}

    public enum Color has copy, drop {
        Red,
        Green,
        Blue { r: u8, g: u8, b: u8 },
    }

    public fun create_coin(value: u64): Coin {
        Coin { value }
    }

    public fun transfer(coin: Coin, to: address, amount: u64): bool {
        let _ = coin;
        let _ = to;
        let _ = amount;
        true
    }

    public fun wrapper_inner<T: store>(w: &Wrapper<T>): &T {
        &w.inner
    }

    public fun swap<T: copy + drop>(a: T, _b: T): T {
        a
    }

    public fun multi_return(): (u64, bool) {
        (42, true)
    }

    fun private_helper(): u64 {
        0
    }

    public(package) fun package_only(x: u64): u64 {
        x + 1
    }

    entry fun do_something(_value: u64) {
    }

    public entry fun public_entry(x: u64, y: u64): u64 {
        x + y
    }
}
