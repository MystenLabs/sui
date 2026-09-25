// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, f: u64 }
public struct Hot {}

public fun new_party(ctx: &mut TxContext) {
    transfer::party_transfer(Obj { id: object::new(ctx), f: 0 }, sui::party::single_owner(ctx.sender()))
}
public fun f(o: &Obj): &u64 { &o.f }
public fun f_mut(o: &mut Obj): &mut u64 { &mut o.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_mut<T>(_: &mut T) {}
public fun heat(_: &Obj): Hot { Hot {} }
public fun cool(h: Hot) { let Hot {} = h; }
entry fun play(_: &u64) {}

//# programmable --sender A
//> 0: test::m::new_party();

//# programmable --sender A --inputs object(2,0) 7
// VALID: a write through a reference into a party object persists
//> 0: test::m::f_mut(Input(0));
//> 1: test::m::write(Result(0), Input(1));

//# view-object 2,0

//# programmable --sender A --inputs object(2,0) 8 @B
// VALID: transferred to an address once the reference is dead, carrying the write
//> 0: test::m::f_mut(Input(0));
//> 1: test::m::write(Result(0), Input(1));
//> 2: TransferObjects([Input(0)], Input(2));

//# view-object 2,0
