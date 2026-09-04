// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun fail() { abort 42 }

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --dev-inspect --inputs object(2,0) 7
// INVALID: abort 42 at command 3 after the write, nothing persisted
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: test::m::fail();

//# view-object 2,0

//# programmable --sender A --dev-inspect --inputs object(2,0) 7
// VALID: a dev-inspect write succeeds but is not committed
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::write(Result(1), Input(1));

//# view-object 2,0

//# programmable --sender A --inputs 100 @A
// INVALID: abort 42 at command 4 after the gas coin was drained through a reference
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: sui::balance::split<sui::sui::SUI>(Result(0), Input(0));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(1));
//> 4: test::m::fail();

//# programmable --sender A --dev-inspect --inputs 100 @A
// INVALID: the same under dev-inspect
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: sui::balance::split<sui::sui::SUI>(Result(0), Input(0));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(1));
//> 4: test::m::fail();
