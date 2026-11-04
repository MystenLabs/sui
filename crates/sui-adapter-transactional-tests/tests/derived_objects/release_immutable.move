// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A frozen derived object cannot be taken by value, so it cannot be released.

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

public fun release(parent: &mut Parent, obj: Obj) {
    let Obj { id } = obj;
    derived_object::release(&mut parent.id, id)
}

public fun freeze_obj(obj: Obj) {
    transfer::public_freeze_object(obj)
}

//# run ex::m::create_parent --sender A

//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::claim(Input(0));
//> ex::m::freeze_obj(Result(0))

//# view-object 3,0

//# programmable --sender A --inputs object(2,0) object(3,0)
//> ex::m::release(Input(0), Input(1))

//# view-object 3,0
