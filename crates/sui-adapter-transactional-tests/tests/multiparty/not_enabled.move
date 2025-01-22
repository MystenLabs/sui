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
    transfer::public_multiparty_transfer(p, vector[@0])
}

public fun pub_multiparty(obj: Pub, p: vector<address>) {
    transfer::public_multiparty_transfer(obj, p)
}

public fun priv_multiparty(obj: Priv, p: vector<address>) {
    transfer::multiparty_transfer(obj, p)
}

//# run ex::m::mint

// Aborts since multiparty transfer is not enabled
//# run ex::m::create_multiparty

// Aborts since multiparty transfer is not enabled
//# run ex::m::priv_multiparty --args object(2,0) vector[@0]

// Aborts since multiparty transfer is not enabled
//# run ex::m::pub_multiparty --args object(2,1) vector[@0]

// Aborts since multiparty transfer is not enabled
//# run sui::transfer::public_multiparty_transfer
//#     --type-args ex::m::Pub
//#     --args object(2,1) vector[@0]
