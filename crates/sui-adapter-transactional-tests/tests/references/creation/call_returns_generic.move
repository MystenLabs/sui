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
public fun id<T>(t: &T): &T { t }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun first_mut<T>(v: &mut vector<T>): &mut T { &mut v[0] }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_elem(v: &vector<u64>, i: u64, e: u64) { assert!(v[i] == e, 0) }
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 4
// VALID: generic identity on an object, an inner struct, and a field
//> 0: test::m::new();
//> 1: test::m::id_mut<test::m::Obj>(Result(0));
//> 2: test::m::inner_mut(Result(1));
//> 3: test::m::id_mut<test::m::Inner>(Result(2));
//> 4: test::m::f_mut(Result(3));
//> 5: test::m::id_mut<u64>(Result(4));
//> 6: test::m::write(Result(5), Input(0));
//> 7: test::m::check(Result(4), Input(0));
//> 8: test::m::delete(Result(0));

//# programmable --inputs 1 0 9
// VALID: generic element borrow of a vector result
//> 0: MakeMoveVec<u64>([Input(0)]);
//> 1: test::m::first_mut<u64>(Result(0));
//> 2: test::m::write(Result(1), Input(2));
//> 3: test::m::check_elem(Result(0), Input(1), Input(2));

//# programmable --inputs 5
// VALID: generic immutable identity on a pure input
//> 0: test::m::id<u64>(Input(0));
//> 1: test::m::check(Result(0), Input(0));
