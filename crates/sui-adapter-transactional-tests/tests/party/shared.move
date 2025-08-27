// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses ex=0x0

//# publish
module ex::m;

public struct S has key, store {
    id: UID,
}

public fun create_and_share(ctx: &mut TxContext) {
    let s = S { id: object::new(ctx) };
    transfer::share_object(s)
}

public fun create_party(ctx: &mut TxContext) {
    let s = S { id: object::new(ctx) };
    transfer::party_transfer(s, sui::party::single_owner(ctx.sender()))
}

// Create a shared object.
//# programmable --sender A
//> ex::m::create_and_share()

//# view-object 2,0

// Verify the shared object can't be transferred to party ownership.
//# programmable --sender A --inputs object(2,0) @A
//> 0: sui::party::single_owner(Input(1));
//> sui::transfer::public_party_transfer<ex::m::S>(Input(0), Result(0))

// Create a party object.
//# programmable --sender A
//> ex::m::create_party()

//# view-object 5,0

// Verify the party object can't be shared.
//# programmable --sender A --inputs object(5,0)
//> sui::transfer::public_share_object<ex::m::S>(Input(0))