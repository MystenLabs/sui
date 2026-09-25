// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 q=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

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
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_elem(v: &vector<u64>, i: u64, e: u64) { assert!(v[i] == e, 0) }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# stage-package
module q::m {
    public fun x(): u64 { 0 }
}

//# programmable --inputs 7
// VALID: MoveCall result
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::delete(Result(0));

//# programmable --sender A --inputs 100 @A
// VALID: SplitCoins result
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: sui::coin::value<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(0)], Input(1));

//# programmable --inputs 1 2 0 9
// VALID: MakeMoveVec result
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: test::m::write(Result(1), Input(3));
//> 3: test::m::check_elem(Result(0), Input(2), Input(3));

//# programmable --sender A --inputs @A
// VALID: Publish result (UpgradeCap)
//> 0: Publish(q, []);
//> 1: test::m::id_mut<sui::package::UpgradeCap>(Result(0));
//> 2: sui::package::version(Result(1));
//> 3: TransferObjects([Result(0)], Input(0));
