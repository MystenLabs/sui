// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Release and re-claim a derived object that is transferred to another object and received.

//# init --addresses ex=0x0 --accounts A

//# publish
module ex::m;

use sui::derived_object;
use sui::transfer::Receiving;

public struct Parent has key { id: UID }

public struct Obj has key, store { id: UID }

public fun create_parent(ctx: &mut TxContext) {
    transfer::transfer(Parent { id: object::new(ctx) }, ctx.sender())
}

public fun claim(parent: &mut Parent): Obj {
    Obj { id: derived_object::claim(&mut parent.id, 0u64) }
}

public fun release(parent: &mut Parent, obj: Obj) {
    let Obj { id } = obj;
    derived_object::release(&mut parent.id, id)
}

public fun send_to_parent(parent: &Parent, obj: Obj) {
    transfer::public_transfer(obj, object::id_address(parent))
}

public fun receive(parent: &mut Parent, obj: Receiving<Obj>): Obj {
    transfer::public_receive(&mut parent.id, obj)
}

//# run ex::m::create_parent --sender A

// X is owned by the parent object.
//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::claim(Input(0));
//> ex::m::send_to_parent(Input(0), Result(0))

//# view-object 3,0

// Receive and release: X is deleted.
//# programmable --sender A --inputs object(2,0) receiving(3,0)
//> 0: ex::m::receive(Input(0), Input(1));
//> ex::m::release(Input(0), Result(0))

// Claim X again and send it to the parent object.
//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::claim(Input(0));
//> ex::m::send_to_parent(Input(0), Result(0))

//# view-object 3,0

// Receive, release, claim, send back in one transaction: X is mutated.
//# programmable --sender A --inputs object(2,0) receiving(3,0)
//> 0: ex::m::receive(Input(0), Input(1));
//> ex::m::release(Input(0), Result(0));
//> 2: ex::m::claim(Input(0));
//> ex::m::send_to_parent(Input(0), Result(2))

//# view-object 3,0

// Receive, release, claim, transfer to A in one transaction: X is mutated with a new owner.
//# programmable --sender A --inputs object(2,0) receiving(3,0) @A
//> 0: ex::m::receive(Input(0), Input(1));
//> ex::m::release(Input(0), Result(0));
//> 2: ex::m::claim(Input(0));
//> TransferObjects([Result(2)], Input(2))

//# view-object 3,0
