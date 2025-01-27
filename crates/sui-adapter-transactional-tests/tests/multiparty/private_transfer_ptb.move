// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses ex=0x0

//# publish
module ex::m;

public struct Priv has key {
    id: UID,
}

public fun mint(ctx: &mut TxContext) {
    let q = Priv { id: object::new(ctx) };
    transfer::transfer(q, ctx.sender());
}

public fun priv(ctx: &mut TxContext): Priv {
    Priv { id: object::new(ctx) }
}

//# run ex::m::mint

// Does not have store
//# programmable --inputs object(2,0) vector[@0]
//> sui::transfer::public_multiparty_transfer<ex::m::Priv>(Input(0), Input(1))

// Does not have store
//# programmable --inputs vector[@0]
//> 0: ex::m::priv();
//> sui::transfer::public_multiparty_transfer<ex::m::Priv>(Result(0), Input(0))

// Private transfer
//# programmable --inputs object(2,0) vector[@0]
//> sui::transfer::multiparty_transfer<ex::m::Priv>(Input(0), Input(1))

// Private transfer
//# programmable --inputs vector[@0]
//> 0: ex::m::priv();
//> sui::transfer::multiparty_transfer<ex::m::Priv>(Result(0), Input(0))
