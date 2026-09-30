// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID }

public fun new(ctx: &mut TxContext): Obj { Obj { id: object::new(ctx) } }

public fun uid_mut(o: &mut Obj): &mut UID { &mut o.id }
public fun write(r: &mut u64, v: u64) { *r = v }

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0) 1 2 9
// INVALID: InvalidReferenceArgument at arg 0 of command 3, remove while a field reference is live
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::dynamic_field::add<u64, u64>(Result(0), Input(1), Input(2));
//> 2: sui::dynamic_field::borrow_mut<u64, u64>(Result(0), Input(1));
//> 3: sui::dynamic_field::remove<u64, u64>(Result(0), Input(1));
//> 4: test::m::write(Result(2), Input(3));

//# programmable --sender A --inputs object(2,0) 1 2
// INVALID: InvalidReferenceArgument at arg 0 of command 3, a fresh `&mut Obj` while a field reference is live
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::dynamic_field::add<u64, u64>(Result(0), Input(1), Input(2));
//> 2: sui::dynamic_field::borrow_mut<u64, u64>(Result(0), Input(1));
//> 3: test::m::uid_mut(Input(0));
//> 4: test::m::write(Result(2), Input(2));
