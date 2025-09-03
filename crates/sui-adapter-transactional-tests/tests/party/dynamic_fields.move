// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses ex=0x0

//# publish
module ex::m;

use sui::dynamic_object_field as ofield;

public struct Parent has key, store {
    id: UID,
}

public struct Child has key, store {
    id: UID,
}

public fun mint(ctx: &mut TxContext) {
    let parent = Parent { id: object::new(ctx) };
    transfer::transfer(parent, ctx.sender());

    let child = Child { id: object::new(ctx) };
    transfer::party_transfer(child, sui::party::single_owner(ctx.sender()))
}

public fun add_df(parent: &mut Parent, child: Child) {
    ofield::add(&mut parent.id, 0, child);
}

public fun remove_df(parent: &mut Parent, ctx: &mut TxContext) {
    let child: Child = ofield::remove(&mut parent.id, 0);
    transfer::transfer(child, ctx.sender())
}

// Create a party object.
//# programmable --sender A
//> ex::m::mint()

// child
//# view-object 2,0

// parent
//# view-object 2,1

// Transfer party object to dynamic field.
//# programmable --sender A --inputs object(2,0) object(2,1)
//> ex::m::add_df(Input(1), Input(0))

//# view-object 2,0

//# view-object 5,0

// Verify the dynamic field child object can't be transferred.
//# programmable --inputs object(2,0) @A --sender A
//> 0: sui::party::single_owner(Input(1));
//> sui::transfer::public_party_transfer<ex::m::A>(Input(0), Result(0))

// Pull the object back out.
//# programmable --sender A --inputs object(2,1)
//> ex::m::remove_df(Input(0))

// Verify it can again be transferred to a different party.
//# programmable --inputs object(2,0) @B --sender A
//> 0: sui::party::single_owner(Input(1));
//> sui::transfer::public_party_transfer<ex::m::Child>(Input(0), Result(0))

//# view-object 2,0
