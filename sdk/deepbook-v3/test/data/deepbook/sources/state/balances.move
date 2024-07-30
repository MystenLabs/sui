// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// `Balances` represents the three assets make up a pool: base, quote, and deep.
/// Whenever funds are moved, they are moved in the form of `Balances`.
module deepbook::balances {
    // === Structs ===
    public struct Balances has store, copy, drop {
        base: u64,
        quote: u64,
        deep: u64,
    }

    // === Public-Package Functions ===
    public(package) fun empty(): Balances {
        Balances { base: 0, quote: 0, deep: 0 }
    }

    public(package) fun new(base: u64, quote: u64, deep: u64): Balances {
        Balances { base: base, quote: quote, deep: deep }
    }

    public(package) fun reset(balances: &mut Balances): Balances {
        let old = *balances;
        balances.base = 0;
        balances.quote = 0;
        balances.deep = 0;

        old
    }

    public(package) fun add_balances(balances: &mut Balances, other: Balances) {
        balances.base = balances.base + other.base;
        balances.quote = balances.quote + other.quote;
        balances.deep = balances.deep + other.deep;
    }

    public(package) fun add_base(balances: &mut Balances, base: u64) {
        balances.base = balances.base + base;
    }

    public(package) fun add_quote(balances: &mut Balances, quote: u64) {
        balances.quote = balances.quote + quote;
    }

    public(package) fun add_deep(balances: &mut Balances, deep: u64) {
        balances.deep = balances.deep + deep;
    }

    public(package) fun base(balances: &Balances): u64 {
        balances.base
    }

    public(package) fun quote(balances: &Balances): u64 {
        balances.quote
    }

    public(package) fun deep(balances: &Balances): u64 {
        balances.deep
    }
}
