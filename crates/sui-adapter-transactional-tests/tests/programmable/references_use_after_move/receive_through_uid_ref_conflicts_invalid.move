// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Parent has key, store { id: UID }
public struct Child has key, store { id: UID, f: u64 }

public fun start(ctx: &mut TxContext) {
    let p = Parent { id: object::new(ctx) };
    let c = Child { id: object::new(ctx), f: 0 };
    transfer::public_transfer(c, object::id_address(&p));
    transfer::public_transfer(p, ctx.sender());
}
public fun uid_mut(p: &mut Parent): &mut UID { &mut p.id }

public fun delete(c: Child) { let Child { id, f: _ } = c; object::delete(id) }

//# programmable --sender A
//> 0: test::m::start();

//# programmable --sender A --inputs object(2,0) receiving(2,1)
// INVALID: ArgumentWithoutValue at arg 1 of command 2, the receiving input received twice through the UID reference
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::transfer::public_receive<test::m::Child>(Result(0), Input(1));
//> 2: sui::transfer::public_receive<test::m::Child>(Result(0), Input(1));
//> 3: test::m::delete(Result(1));
//> 4: test::m::delete(Result(2));
