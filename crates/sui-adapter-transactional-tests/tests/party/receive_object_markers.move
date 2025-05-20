// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses ex=0x0

//# publish
module ex::m;

public struct A has key, store {
    id: UID,
}

public struct B has key, store {
    id: UID,
}

public fun mint(ctx: &mut TxContext) {
    let a = A { id: object::new(ctx) };
    let a_address = object::id_address(&a);
    let b1 = B { id: object::new(ctx) };
    let b2 = B { id: object::new(ctx) };
    transfer::public_transfer(a, tx_context::sender(ctx));
    transfer::public_transfer(b1, a_address);
    transfer::public_transfer(b2, a_address);
}

public fun receive(parent: &mut A, x: sui::transfer::Receiving<B>, addr: address) {
    let b = transfer::receive(&mut parent.id, x);
    transfer::public_transfer(b, addr);
}

public fun delete(b: B) {
    let B { id } = b;
    object::delete(id);
}

// Setup:
//# run ex::m::mint

// a
//# view-object 2,0

// b1
//# view-object 2,1

// b2
//# view-object 2,2



// b1:
// 1. Receive the object
// 2. Transfer to party
// 3. Verify cannot receive again at the old version

//# run ex::m::receive --args object(2,0) receiving(2,1) @A

//# programmable --inputs object(2,1) @A --sender A
//> 0: sui::party::single_owner(Input(1));
//> sui::transfer::public_party_transfer<ex::m::B>(Input(0), Result(0))


//# run ex::m::receive --args object(2,0) receiving(2,1)@3 @A




// b2:
// 1. Receive the object
// 2. Transfer to party
// 3. Delete
// 4. Verify cannot receive again at the old version

//# run ex::m::receive --args object(2,0) receiving(2,2) @A

//# programmable --inputs object(2,2) @A --sender A
//> 0: sui::party::single_owner(Input(1));
//> sui::transfer::public_party_transfer<ex::m::B>(Input(0), Result(0))

//# run ex::m::delete --args object(2,2) --sender A

//# run ex::m::receive --args object(2,0) receiving(2,2)@3 @A
