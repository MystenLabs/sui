// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that receiving an object with the correct type, then attempting to receive the same object
// with the wrong phantom type

//# init --addresses tto=0x0

//# publish
module tto::m1;

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
    transfer::public_transfer(a, tx_context::sender(ctx));
    transfer::public_transfer(b, a_address);
}

public fun receive_a(parent: &mut A, rec: Receiving<A>, ctx: &mut TxContext) {
    let a = transfer::receive(&mut parent.id, rec);
    transfer::public_transfer(a, ctx.sender());
}

public fun receive_b(parent: &mut A, rec: Receiving<B>, ctx: &mut TxContext) {
    let b = transfer::receive(&mut parent.id, rec);
    transfer::public_transfer(b, ctx.sender());
}

//# run tto::m1::start

//# view-object 2,0

//# view-object 2,1

//# programmable --inputs object(2,0) receiving(2,1)
// receive correct then wrong
//> tto::m1::receive_b(Input(0), Input(1));
//> tto::m1::receive_a(Input(0), Input(1));
