// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that shared objects can

//# init --addresses t1=0x0 t2=0x0

//# publish

module t2::o2 {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;

    struct Obj2 has key, store {
        id: UID,
    }

    public entry fun create(ctx: &mut TxContext) {
        let o = Obj2 { id: object::new(ctx) };
        transfer::share_object(o)
    }

    public entry fun consume_o2(o2: Obj2) {
        let Obj2 { id } = o2;
        object::delete(id);
    }
}

//# publish --dependencies t2

module t1::o1 {
    use t2::o2::{Self, Obj2};

    public entry fun consume_o2(o2: Obj2) {
        o2::consume_o2(o2);
    }
}


//# run t2::o2::create

//# view-object 108

//# run t1::o1::consume_o2 --args object(108)

//# run t2::o2::consume_o2 --args object(108)
