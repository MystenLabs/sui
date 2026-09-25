// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, f: u64 }

public fun share(ctx: &mut TxContext) {
    transfer::public_share_object(Obj { id: object::new(ctx), f: 0 })
}
public fun f_mut(o: &mut Obj): &mut u64 { &mut o.f }
public fun f(o: &Obj): &u64 { &o.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun two_mut<T>(_: &mut T, _: &mut T) {}

//# programmable --sender A
//> 0: test::m::share();

//# programmable --sender A --inputs object(0,0)
// INVALID: the input check rejects the gas coin as an object input
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Input(0));
//> 2: test::m::two_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0), Result(1));

//# programmable --sender A --inputs object(2,0) immshared(2,0) 7
// INVALID: the input check rejects one shared object as two inputs
//> 0: test::m::f_mut(Input(0));
//> 1: test::m::f(Input(1));
//> 2: test::m::write(Result(0), Input(2));
//> 3: test::m::check(Result(1), Input(2));
