// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A shared derived object cannot be released. It can still be deleted, which keeps its claim.

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

public fun share(obj: Obj) {
    transfer::public_share_object(obj)
}

public fun delete(obj: Obj) {
    let Obj { id } = obj;
    id.delete()
}

public fun assert_claimed(parent: &Parent, claimed: bool) {
    assert!(derived_object::exists(&parent.id, 0u64) == claimed)
}

//# run ex::m::create_parent --sender A

//# programmable --sender A --inputs object(2,0)
//> 0: ex::m::claim(Input(0));
//> ex::m::share(Result(0))

//# view-object 3,0

// Releasing a shared object aborts.
//# programmable --sender A --inputs object(2,0) object(3,0)
//> ex::m::release(Input(0), Input(1))

// Releasing, re-claiming, and re-sharing in one transaction also aborts at the release.
//# programmable --sender A --inputs object(2,0) object(3,0)
//> ex::m::release(Input(0), Input(1));
//> 1: ex::m::claim(Input(0));
//> ex::m::share(Result(1))

//# view-object 3,0

// Deleting the shared object is allowed and keeps the claim.
//# run ex::m::delete --sender A --args object(3,0)

//# run ex::m::assert_claimed --sender A --args object(2,0) true

// An owned derived object under a second parent.
//# run ex::m::create_parent --sender A

//# programmable --sender A --inputs object(10,0) @A
//> 0: ex::m::claim(Input(0));
//> TransferObjects([Result(0)], Input(1))

// Release, re-claim, and share in one transaction: the re-claimed object is not new, so sharing
// aborts.
//# programmable --sender A --inputs object(10,0) object(11,0)
//> ex::m::release(Input(0), Input(1));
//> 1: ex::m::claim(Input(0));
//> ex::m::share(Result(1))

// Release, then re-claim and share in a later transaction: the object is new, so sharing succeeds.
//# programmable --sender A --inputs object(10,0) object(11,0)
//> ex::m::release(Input(0), Input(1))

//# programmable --sender A --inputs object(10,0)
//> 0: ex::m::claim(Input(0));
//> ex::m::share(Result(0))

//# view-object 11,0

// Now that it is shared, it cannot be released.
//# programmable --sender A --inputs object(10,0) object(11,0)
//> ex::m::release(Input(0), Input(1))
