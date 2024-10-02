// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses tto=0x0 --accounts A

//# publish
module tto::M1 {
    use sui::transfer::Receiving;

    public struct A has key, store {
        id: UID,
    }

    public struct B has key, store {
        id: UID,
    }

    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        let b = B { id: object::new(ctx) };
        transfer::public_share_object(a);
        transfer::public_transfer(b, a_address);
    }


    public fun middle(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        transfer::public_transfer(a, tx_context::sender(ctx));
    }

    public entry fun send_back(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        let parent_address = object::id_address(parent);
        transfer::public_transfer(b, parent_address);
    }
}

//# run tto::M1::start

//# run tto::M1::middle --sender A

//# view-object 2,0

//# view-object 2,1

// Can receive the object and then send it
//# run tto::M1::send_back --args object(2,0) receiving(2,1)

//# view-object 2,0

//# view-object 2,1

// Can no longer receive that object at the previous version number
//# run tto::M1::send_back --args object(2,0) receiving(2,1)@3

// Can receive the object at the new version number
//# run tto::M1::send_back --args object(2,0) receiving(2,1)@4

// Cannot try and receive the object with an invalid owner even if it has the right type
//# run tto::M1::send_back --summarize --args object(3,0) receiving(2,1)@6 --sender A

// Can run still receive and send back so state is all good still, and version number hasn't been incremented for the object
//# run tto::M1::send_back --args object(2,0) receiving(2,1)@6
