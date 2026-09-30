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

public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }
public fun delete_all(mut v: vector<Obj>) {
    while (!v.is_empty()) { delete(v.pop_back()) };
    v.destroy_empty()
}

//# programmable --sender A --inputs 0 9 @A
// INVALID: CannotMoveBorrowedValue at arg 0 of command 3, an object put in a vector while borrowed
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: MakeMoveVec<test::m::Obj>([Result(0), Result(1)]);
//> 4: test::m::use_mut<test::m::Inner>(Result(2));
//> 5: std::vector::pop_back<test::m::Obj>(Result(3));
//> 6: std::vector::pop_back<test::m::Obj>(Result(3));
//> 7: std::vector::destroy_empty<test::m::Obj>(Result(3));
//> 8: TransferObjects([Result(5), Result(6)], Input(2));

//# programmable --sender A --inputs 0
// INVALID: CannotMoveBorrowedValue at arg 0 of command 5, the vector consumed while an element chain is live
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: MakeMoveVec<test::m::Obj>([Result(0), Result(1)]);
//> 3: std::vector::borrow_mut<test::m::Obj>(Result(2), Input(0));
//> 4: test::m::inner_mut(Result(3));
//> 5: test::m::delete_all(Result(2));
//> 6: test::m::use_mut<test::m::Inner>(Result(4));
