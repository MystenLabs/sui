// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Release and re-claim an address-owned derived object, across transactions and within one
// transaction.

//# init --addresses ex=0x0 --accounts A

//# publish
module ex::m;

use sui::derived_object;

public struct Parent has key { id: UID }

public struct Obj has key, store { id: UID }

public struct Obj2 has key, store { id: UID }

public fun create_parent(ctx: &mut TxContext) {
    transfer::transfer(Parent { id: object::new(ctx) }, ctx.sender())
}

public fun claim(parent: &mut Parent): Obj {
    Obj { id: derived_object::claim(&mut parent.id, 0u64) }
}

public fun claim2(parent: &mut Parent): Obj2 {
    Obj2 { id: derived_object::claim(&mut parent.id, 0u64) }
}

public fun release(parent: &mut Parent, obj: Obj) {
    let Obj { id } = obj;
    derived_object::release(&mut parent.id, id)
}

public fun release2(parent: &mut Parent, obj: Obj2) {
    let Obj2 { id } = obj;
    derived_object::release(&mut parent.id, id)
}

public fun assert_claimed(parent: &Parent, claimed: bool) {
    assert!(derived_object::exists(&parent.id, 0u64) == claimed)
}

//# run ex::m::create_parent --sender A

// Claim, release, claim, release, claim across transactions.
//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# view-object 3,0

//# programmable --sender A --inputs object(2,0) object(3,0)
//> ex::m::release(Input(0), Input(1))

//# run ex::m::assert_claimed --sender A --args object(2,0) false

//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# view-object 3,0

//# programmable --sender A --inputs object(2,0) object(3,0)
//> ex::m::release(Input(0), Input(1))

//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# view-object 3,0

// Release then claim an input in one transaction: X is mutated.
//# programmable --sender A --inputs object(2,0) object(3,0) @A
//> ex::m::release(Input(0), Input(1));
//> 1: ex::m::claim(Input(0));
//> TransferObjects([Result(1)], Input(2))

//# view-object 3,0

// The same, re-claimed as a different type.
//# programmable --sender A --inputs object(2,0) object(3,0) @A
//> ex::m::release(Input(0), Input(1));
//> 1: ex::m::claim2(Input(0));
//> TransferObjects([Result(1)], Input(2))

//# view-object 3,0

// Release, claim, release an input in one transaction: X is deleted.
//# programmable --sender A --inputs object(2,0) object(3,0)
//> ex::m::release2(Input(0), Input(1));
//> 1: ex::m::claim(Input(0));
//> ex::m::release(Input(0), Result(1))

//# run ex::m::assert_claimed --sender A --args object(2,0) false

// Claim then release in one transaction: no effect on X.
//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::claim(Input(0));
//> ex::m::release(Input(0), Result(0))

// Claim, release, claim in one transaction: X is created.
//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> ex::m::release(Input(0), Result(0));
//> 2: ex::m::claim(Input(0));
//> TransferObjects([Result(2)], Input(1))

//# view-object 3,0

//# run ex::m::assert_claimed --sender A --args object(2,0) true
