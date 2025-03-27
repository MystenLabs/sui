// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses ex=0x0

//# publish
module ex::m;

public struct Pub has key, store {
    id: UID,
}

public struct Priv has key {
    id: UID,
}

public fun mint(ctx: &mut TxContext) {
    let p = Pub { id: object::new(ctx) };
    let q = Priv { id: object::new(ctx) };
    transfer::public_transfer(p, ctx.sender());
    transfer::transfer(q, ctx.sender());
}

public fun create_multiparty(ctx: &mut TxContext) {
    let p = Pub { id: object::new(ctx) };
    transfer::public_multiparty_transfer(p, sui::multiparty::single_owner(@0))
}

public fun pub_multiparty(obj: Pub, p: sui::multiparty::Multiparty) {
    transfer::public_multiparty_transfer(obj, p)
}

public fun priv_multiparty(obj: Priv, p: sui::multiparty::Multiparty) {
    transfer::multiparty_transfer(obj, p)
}

//# run ex::m::mint

// Aborts since multiparty transfer is not enabled
//# run ex::m::create_multiparty

// Aborts since multiparty transfer is not enabled
//# programmable --inputs object(2,0) @0
//> 0: sui::multiparty::single_owner(Input(1));
//> ex::m::priv_multiparty(Input(0), Result(0))


// Aborts since multiparty transfer is not enabled
//# programmable --inputs object(2,1) @0
//> 0: sui::multiparty::single_owner(Input(1));
//> ex::m::pub_multiparty(Input(0), Result(0))

// Aborts since multiparty transfer is not enabled
//# programmable --inputs object(2,1) @0
//> 0: sui::multiparty::single_owner(Input(1));
//> sui::transfer::public_multiparty_transfer<ex::m::Pub>(Input(0), Result(0))
