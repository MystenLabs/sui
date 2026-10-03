// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::transfer::Receiving;

public struct Parent has key, store { id: UID }
public struct Child has key, store { id: UID }

public fun start(ctx: &mut TxContext) {
    let p = Parent { id: object::new(ctx) };
    let c = Child { id: object::new(ctx) };
    transfer::public_transfer(c, object::id_address(&p));
    transfer::public_transfer(p, ctx.sender());
}

public fun v_then_r(_: Receiving<Child>, _: &Receiving<Child>) { abort 0 }

//# programmable --sender A
//> 0: test::m::start();

//# programmable --sender A --inputs receiving(2,1)
// INVALID: ArgumentWithoutValue at arg 1 of command 0
//> 0: test::m::v_then_r(Input(0), Input(0));
