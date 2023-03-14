// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// similar to dynamic_field_tests but over multiple transactions, as this uses a different code path
// test duplicate add

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {

    use sui::dynamic_field::add;
    use sui::object;
    use sui::tx_context::{sender, TxContext};

    struct Obj has key {
        id: object::UID,
    }

    public entry fun add_n_items(n: u64, ctx: &mut TxContext) {
        let i = 0;
        while (i < n) {
            let id = object::new(ctx);
            add<u64, u64>(&mut id, i, i);
            sui::transfer::transfer(Obj { id }, sender(ctx));

            i = i + 1;
        };
    }
}

//# run a::m::add_n_items --sender A --args 100

//# run a::m::add_n_items --sender A --args 1000

//# run a::m::add_n_items --sender A --args 1025
