// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses tto=0x0 --accounts A

//# publish
module tto::M1 {
    use sui::transfer::Receiving;
    use sui::dynamic_object_field as dof;

    const KEY: u64 = 0;

    public struct A has key, store {
        id: UID,
        value: u64,
    }

    public fun start(ctx: &mut TxContext) {
        let mut a = A { id: object::new(ctx), value: 0 };
        dof::add(&mut a.id, KEY, A { id: object::new(ctx), value: 0 });
        transfer::public_transfer(a, tx_context::sender(ctx));
    }

    public entry fun receive(parent: &mut A, x: Receiving<A>) {
        let b = transfer::receive(&mut parent.id, x);
        dof::add(&mut parent.id, KEY, b);
    }
}

//# run tto::M1::start --sender A

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

// Try to receive an object with an object
//# run tto::M1::receive --args object(2,2) receiving(2,1) --sender A

// Try to receive another object with an object owner
//# run tto::M1::receive --args object(2,2) receiving(2,0) --sender A
