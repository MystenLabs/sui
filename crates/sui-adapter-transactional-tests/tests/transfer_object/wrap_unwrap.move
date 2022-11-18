// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// test impact to versions for wrapping and unwrapping an object

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {
    use sui::object;
    use sui::tx_context::{Self, TxContext};

    struct S has key, store {
        id: object::UID,
    }

    struct T has key, store {
        id: object::UID,
        s: S,
    }

    entry fun mint(ctx: &mut TxContext) {
        sui::transfer::transfer(
            S { id: object::new(ctx) },
            tx_context::sender(ctx),
        );
    }

    entry fun wrap(s: S, ctx: &mut TxContext) {
        sui::transfer::transfer(
            T { id: object::new(ctx), s },
            tx_context::sender(ctx),
        );
    }

    entry fun unwrap(t: T, ctx: &mut TxContext) {
        let T { id, s } = t;
        object::delete(id);
        sui::transfer::transfer(s, tx_context::sender(ctx));
    }
}

//# run a::m::mint --sender A

//# view-object 106

//# run a::m::wrap --sender A --args object(106)

//# view-object 108

//# run a::m::unwrap --sender A --args object(108)

//# view-object 106
