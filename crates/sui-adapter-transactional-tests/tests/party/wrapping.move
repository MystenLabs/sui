// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses ex=0x0

//# publish
module ex::m;

public struct A has key, store {
    id: UID,
}

public struct AWrapper has key, store {
    id: UID,
    a: A,
}

public fun create_party(ctx: &mut TxContext) {
    let a = A { id: object::new(ctx) };
    transfer::party_transfer(a, sui::party::single_owner(ctx.sender()))
}

public fun wrap(a: A, ctx: &mut TxContext) {
    let wrapper = AWrapper { id: object::new(ctx), a };
    transfer::party_transfer(wrapper, sui::party::single_owner(ctx.sender()))
}

public fun unwrap(wrapper: AWrapper, ctx: &mut TxContext) {
    let AWrapper { id, a } = wrapper;
    transfer::party_transfer(a, sui::party::single_owner(tx_context::sender(ctx)));
    object::delete(id)
}

// Create a party object.
//# programmable --sender A
//> ex::m::create_party()

//# view-object 2,0

// Wrap the party object.
//# programmable --sender A --inputs object(2,0)
//> ex::m::wrap(Input(0))

//# view-object 2,0

//# view-object 4,0

// Verify the wrapped object can't be transferred.
//# programmable --inputs object(2,0) @A --sender A
//> 0: sui::party::single_owner(Input(1));
//> sui::transfer::public_party_transfer<ex::m::A>(Input(0), Result(0))

// Unwrap the object.
//# programmable --sender A --inputs object(4,0)
//> ex::m::unwrap(Input(0))

// Verify it can again be transferred to a different party.
//# programmable --inputs object(2,0) @B --sender A
//> 0: sui::party::single_owner(Input(1));
//> sui::transfer::public_party_transfer<ex::m::A>(Input(0), Result(0))

//# view-object 2,0
