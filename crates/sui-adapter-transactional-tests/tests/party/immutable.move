// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses ex=0x0

//# publish
module ex::m;

public struct A has key, store {
    id: UID,
}

public fun create_party(ctx: &mut TxContext) {
    let a = A { id: object::new(ctx) };
    transfer::party_transfer(a, sui::party::single_owner(ctx.sender()))
}

// Create a party object.
//# programmable --sender A
//> ex::m::create_party()

//# view-object 2,0

// Freeze the party object.
//# programmable --sender A --inputs object(2,0)
//> sui::transfer::public_freeze_object<ex::m::A>(Input(0))

//# view-object 2,0

// Verify the Immutalbe object can't be transferred back to party ownership.
//# programmable --sender A --inputs object(2,0) @A
//> 0: sui::party::single_owner(Input(1));
//> sui::transfer::public_party_transfer<ex::m::A>(Input(0), Result(0))
