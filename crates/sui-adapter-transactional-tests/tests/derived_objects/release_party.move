// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Release and re-claim a party-owned derived object.

//# init --addresses ex=0x0 --accounts A B

//# publish
module ex::m;

use sui::derived_object;
use sui::party;

public struct Parent has key { id: UID }

public struct Obj has key, store { id: UID, value: u64 }

public fun create_parent(ctx: &mut TxContext) {
    transfer::transfer(Parent { id: object::new(ctx) }, ctx.sender())
}

public fun claim(parent: &mut Parent): Obj {
    Obj { id: derived_object::claim(&mut parent.id, 0u64), value: 0 }
}

public fun release(parent: &mut Parent, obj: Obj) {
    let Obj { id, value: _ } = obj;
    derived_object::release(&mut parent.id, id)
}

public fun to_party(obj: Obj, owner: address) {
    transfer::public_party_transfer(obj, party::single_owner(owner))
}

public fun set(obj: &mut Obj, value: u64) {
    obj.value = value
}

//# run ex::m::create_parent --sender A

//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> ex::m::to_party(Result(0), Input(1))

//# view-object 3,0

// Release a party object: X is deleted.
//# programmable --sender A --inputs object(2,0) object(3,0)
//> ex::m::release(Input(0), Input(1))

// Re-claim as party in a later transaction: X starts a new consensus stream.
//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> ex::m::to_party(Result(0), Input(1))

//# view-object 3,0

//# run ex::m::set --sender A --args object(3,0) 1

// Release, claim, party transfer to the same owner in one transaction: the start version is kept.
//# programmable --sender A --inputs object(2,0) object(3,0) @A
//> ex::m::release(Input(0), Input(1));
//> 1: ex::m::claim(Input(0));
//> ex::m::to_party(Result(1), Input(2))

//# view-object 3,0

//# run ex::m::set --sender A --args object(3,0) 2

// Release, claim, party transfer to a different owner in one transaction: the start version
// changes.
//# programmable --sender A --inputs object(2,0) object(3,0) @B
//> ex::m::release(Input(0), Input(1));
//> 1: ex::m::claim(Input(0));
//> ex::m::to_party(Result(1), Input(2))

//# view-object 3,0

//# run ex::m::set --sender B --args object(3,0) 3

//# view-object 3,0
