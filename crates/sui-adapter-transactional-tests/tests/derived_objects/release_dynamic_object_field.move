// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Release and re-claim a derived object that is stored as a dynamic object field.

//# init --addresses ex=0x0 --accounts A

//# publish
module ex::m;

use sui::derived_object;
use sui::dynamic_object_field as dof;

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

public fun add_dof(parent: &mut Parent, obj: Obj) {
    dof::add(&mut parent.id, b"x", obj)
}

public fun remove_dof(parent: &mut Parent): Obj {
    dof::remove(&mut parent.id, b"x")
}

//# run ex::m::create_parent --sender A

//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::claim(Input(0));
//> ex::m::add_dof(Input(0), Result(0))

//# view-object 3,0

// Remove the dynamic object field and release: X and its field are deleted.
//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::remove_dof(Input(0));
//> ex::m::release(Input(0), Result(0))

// Claim X again as a dynamic object field: X and its field are created.
//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::claim(Input(0));
//> ex::m::add_dof(Input(0), Result(0))

// Remove, release, claim, add back in one transaction: X ends unchanged, so it is not in the
// effects.
//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::remove_dof(Input(0));
//> ex::m::release(Input(0), Result(0));
//> 2: ex::m::claim(Input(0));
//> ex::m::add_dof(Input(0), Result(2))

//# view-object 3,0

// Remove, release, claim, transfer to A in one transaction: X is mutated with a new owner and its
// field is deleted.
//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::remove_dof(Input(0));
//> ex::m::release(Input(0), Result(0));
//> 2: ex::m::claim(Input(0));
//> TransferObjects([Result(2)], Input(1))

//# view-object 3,0
