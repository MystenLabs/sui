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
public fun check_f(o: &Obj, v: u64) { assert!(o.inner.f == v, 0) }
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }
public fun delete_all(mut v: vector<Obj>) {
    while (!v.is_empty()) { delete(v.pop_back()) };
    v.destroy_empty()
}

//# programmable --sender A --inputs 0 9 @A
// VALID: write through the element chain, then pop both and transfer
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: MakeMoveVec<test::m::Obj>([Result(0), Result(1)]);
//> 3: std::vector::borrow_mut<test::m::Obj>(Result(2), Input(0));
//> 4: test::m::inner_mut(Result(3));
//> 5: test::m::f_mut(Result(4));
//> 6: test::m::write(Result(5), Input(1));
//> 7: std::vector::pop_back<test::m::Obj>(Result(2));
//> 8: std::vector::pop_back<test::m::Obj>(Result(2));
//> 9: std::vector::destroy_empty<test::m::Obj>(Result(2));
//> 10: TransferObjects([Result(7), Result(8)], Input(2));

//# view-object 2,0

//# view-object 2,1

//# programmable --sender A --inputs 0 9 @A
// INVALID: InvalidReferenceArgument at arg 0 of command 6, pop while the element chain is live
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: MakeMoveVec<test::m::Obj>([Result(0), Result(1)]);
//> 3: std::vector::borrow_mut<test::m::Obj>(Result(2), Input(0));
//> 4: test::m::inner_mut(Result(3));
//> 5: test::m::f_mut(Result(4));
//> 6: std::vector::pop_back<test::m::Obj>(Result(2));
//> 7: test::m::write(Result(5), Input(1));
//> 8: std::vector::pop_back<test::m::Obj>(Result(2));
//> 9: std::vector::destroy_empty<test::m::Obj>(Result(2));
//> 10: TransferObjects([Result(6), Result(8)], Input(2));

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

//# programmable --sender A --inputs 0 9 @A
// VALID: pop an element once its reference is dead, then borrow and write the popped object
//> 0: test::m::new();
//> 1: MakeMoveVec<test::m::Obj>([Result(0)]);
//> 2: std::vector::borrow_mut<test::m::Obj>(Result(1), Input(0));
//> 3: test::m::use_mut<test::m::Obj>(Result(2));
//> 4: std::vector::pop_back<test::m::Obj>(Result(1));
//> 5: test::m::inner_mut(Result(4));
//> 6: test::m::f_mut(Result(5));
//> 7: test::m::write(Result(6), Input(1));
//> 8: test::m::check_f(Result(4), Input(1));
//> 9: TransferObjects([Result(4)], Input(2));
//> 10: std::vector::destroy_empty<test::m::Obj>(Result(1));

//# view-object 8,0
