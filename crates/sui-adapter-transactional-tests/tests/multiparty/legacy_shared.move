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

public fun create_priv(ctx: &mut TxContext): Priv {
    Priv { id: object::new(ctx) }
}

public fun create_pub(ctx: &mut TxContext): Pub {
    Pub { id: object::new(ctx) }
}

public fun priv_multiparty(obj: Priv, p: sui::multiparty::Multiparty) {
    transfer::multiparty_transfer(obj, p)
}

//# run ex::m::mint

// Aborts since legacy_shared  does not yet support "upgrades"
//# programmable --inputs object(2,0)
//> 0: sui::multiparty::legacy_shared();
//> ex::m::priv_multiparty(Input(0), Result(0))

// Aborts since legacy_shared  does not yet support "upgrades"
//# programmable --inputs object(2,1)
//> 0: sui::multiparty::legacy_shared();
//> sui::transfer::public_multiparty_transfer<ex::m::Pub>(Input(0), Result(0))

// creates shared objects
//# programmable
//> 0: sui::multiparty::legacy_shared();
//> 1: ex::m::create_priv();
//> 2: ex::m::priv_multiparty(Result(1), Result(0));
//> 3: ex::m::create_pub();
//> sui::transfer::public_multiparty_transfer<ex::m::Pub>(Result(3), Result(0))

//# view-object 5,0

//# view-object 5,1
