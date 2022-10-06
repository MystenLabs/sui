// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::wallet {
    use sui::tx_context::{Self, TxContext};
    use sui::coin::{Self, Coin};
    use sui::transfer;
    use std::vector;

    /// For when trying to split a coin more times than its balance allows.
    const ENotEnough: u64 = 0;

    /// For when invalid arguments are passed to a function.
    const EInvalidArg: u64 = 1;

    /// Split coin `self` to two coins, one with balance `split_amount`,
    /// and the remaining balance is left is `self`.
    public entry fun split<T>(self: &mut Coin<T>, split_amount: u64, ctx: &mut TxContext) {
        transfer::transfer(
            coin::split(self, split_amount, ctx),
            tx_context::sender(ctx)
        )
    }

    /// Split coin `self` into multiple coins, each with balance specified
    /// in `split_amounts`. Remaining balance is left in `self`.
    public entry fun split_vec<T>(self: &mut Coin<T>, split_amounts: vector<u64>, ctx: &mut TxContext) {
        let i = 0;
        let len = vector::length(&split_amounts);
        while (i < len) {
            split(self, *vector::borrow(&split_amounts, i), ctx);
            i = i + 1;
        };
    }

    /// Split coin `self` into `n` coins with equal balances. If the balance is
    /// not evenly divisible by `n`, the remainder is left in `self`. Return
    /// newly created coins.
    public fun split_n_to_vec<T>(self: &mut Coin<T>, n: u64, ctx: &mut TxContext): vector<Coin<T>> {
        assert!(n > 0, EInvalidArg);
        assert!(n <= coin::value(self), ENotEnough);
        let vec = vector::empty<Coin<T>>();
        let i = 0;
        let split_amount = coin::value(self) / n;
        while (i < n - 1) {
            vector::push_back(&mut vec, coin::split(self, split_amount, ctx));
            i = i + 1;
        };
        vec
    }

    /// Split coin `self` into `n` coins with equal balances. If the balance is
    /// not evenly divisible by `n`, the remainder is left in `self`.
    public entry fun split_n<T>(self: &mut Coin<T>, n: u64, ctx: &mut TxContext) {
        let vec: vector<Coin<T>> = split_n_to_vec(self, n, ctx);
        let i = 0;
        let len = vector::length(&vec);
        while (i < len) {
            transfer::transfer(vector::pop_back(&mut vec), tx_context::sender(ctx));
            i = i + 1;
        };
        vector::destroy_empty(vec);
    }

    /// Join everything in `coins` with `self`
    public entry fun join_vec<T>(self: &mut Coin<T>, coins: vector<Coin<T>>) {
        let i = 0;
        let len = vector::length(&coins);
        while (i < len) {
            let coin = vector::remove(&mut coins, i);
            coin::join(self, coin);
            i = i + 1
        };
        // safe because we've drained the vector
        vector::destroy_empty(coins)
    }
}
