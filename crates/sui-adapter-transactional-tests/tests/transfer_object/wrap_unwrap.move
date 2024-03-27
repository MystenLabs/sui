// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// test impact to versions for wrapping and unwrapping an object

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {

    public struct S has key, store {
        id: object::UID,
    }

    public struct T has key, store {
        id: object::UID,
        s: S,
    }

    entry fun mint(ctx: &mut TxContext) {
        sui::transfer::public_transfer(
            S { id: object::new(ctx) },
            tx_context::sender(ctx),
        );
    }

    entry fun wrap(s: S, ctx: &mut TxContext) {
        sui::transfer::public_transfer(
            T { id: object::new(ctx), s },
            tx_context::sender(ctx),
        );
    }

    entry fun unwrap(t: T, ctx: &mut TxContext) {
        let T { id, s } = t;
        object::delete(id);
        sui::transfer::public_transfer(s, tx_context::sender(ctx));
    }
}

//# run a::m::mint --sender A

//# view-object 2,0

//# run a::m::wrap --sender A --args object(2,0)

//# view-object 4,0

//# run a::m::unwrap --sender A --args object(4,0)

//# view-object 2,0
