// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


//# init --addresses a=0x0 --accounts A --max-gas 100000000000000

//# publish
module a::m {

    use sui::dynamic_field::add;

    public struct Obj has key {
        id: object::UID,
    }

    public entry fun add_n_items(n: u64, ctx: &mut TxContext) {
        let mut i = 0;
        while (i < n) {
            let mut id = object::new(ctx);
            add<u64, u64>(&mut id, i, i);
            sui::transfer::transfer(Obj { id }, ctx.sender());

            i = i + 1;
        };
    }
}

//# run a::m::add_n_items --sender A --args 100 --gas-budget 1000000000000 --summarize

//# run a::m::add_n_items --sender A --args 1000 --gas-budget 1000000000000 --summarize

//# run a::m::add_n_items --sender A --args 1025 --gas-budget 1000000000000

//# run a::m::add_n_items --sender A --args 1025 --gas-budget 100000000000000
