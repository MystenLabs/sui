// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A

//# publish --sender A

module test::coin_in_vec {
    use std::vector;
    use sui::coin::Coin;
    use sui::object::{Self, UID};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Wrapper has key {
        id: UID,
        coins: vector<Coin<SUI>>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::transfer(Wrapper { id: object::new(ctx), coins: vector[] }, tx_context::sender(ctx));
    }

    public fun deposit(wrapper: &mut Wrapper, c: Coin<SUI>) {
        vector::push_back(&mut wrapper.coins, c)
    }

    public fun withdraw(wrapper: &mut Wrapper, ctx: &mut TxContext) {
        transfer::public_transfer(vector::pop_back(&mut wrapper.coins), tx_context::sender(ctx))
    }
}

//# run test::coin_in_vec::deposit --args --args object(105) object(104) --sender A

//# run test::coin_in_vec::withdraw --args object(105) --sender A
