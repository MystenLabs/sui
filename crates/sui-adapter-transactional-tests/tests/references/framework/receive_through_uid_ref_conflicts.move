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
public fun f_mut(c: &mut Child): &mut u64 { &mut c.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_mut<T>(_: &mut T) {}
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

//# programmable --sender A --inputs object(2,0) receiving(2,1)
// INVALID: InvalidReferenceArgument at arg 0 of command 2, a fresh `&mut Parent` while the UID reference is live
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::transfer::public_receive<test::m::Child>(Result(0), Input(1));
//> 2: test::m::uid_mut(Input(0));
//> 3: test::m::use_mut<sui::object::UID>(Result(0));
//> 4: test::m::delete(Result(1));

//# programmable --sender A --inputs object(2,0) receiving(2,1) 7 @A
// VALID: receive through the UID reference, then borrow and write the received child before transferring it
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::transfer::public_receive<test::m::Child>(Result(0), Input(1));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(2));
//> 4: TransferObjects([Result(1)], Input(3));

//# view-object 2,1
