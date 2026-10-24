// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `release` aborts for a UID that is not claimed under the given parent.

//# init --addresses ex=0x0 --accounts A

//# publish
module ex::m;

use sui::derived_object;

public struct Parent has key { id: UID }

public struct Obj has key, store { id: UID }

public fun create_parent(ctx: &mut TxContext) {
    transfer::transfer(Parent { id: object::new(ctx) }, ctx.sender())
}

public fun claim(parent: &mut Parent): Obj {
    Obj { id: derived_object::claim(&mut parent.id, 0u64) }
}

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx) }
}

public fun release(parent: &mut Parent, obj: Obj) {
    let Obj { id } = obj;
    derived_object::release(&mut parent.id, id)
}

//# run ex::m::create_parent --sender A

//# run ex::m::create_parent --sender A

//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> TransferObjects([Result(0)], Input(1))

// Released under a different parent.
//# programmable --sender A --inputs object(3,0) object(4,0)
//> ex::m::release(Input(0), Input(1))

// A UID from `object::new`.
//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::new();
//> ex::m::release(Input(0), Result(0))

// Both failures leave the derived object in place.
//# view-object 4,0
