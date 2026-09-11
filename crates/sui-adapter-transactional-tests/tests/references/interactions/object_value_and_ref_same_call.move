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
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun take_then_mut(o: Obj, _: &mut Inner) { delete(o) }
public fun mut_then_take(_: &mut Inner, o: Obj) { delete(o) }
public fun take_then_imm(o: Obj, _: &Inner) { delete(o) }
public fun imm_then_take(_: &Inner, o: Obj) { delete(o) }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, object input by value then its `&mut Inner`
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::take_then_mut(Input(0), Result(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, `&mut Inner` then the object input by value
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::mut_then_take(Result(0), Input(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, object input by value then its `&Inner`
//> 0: test::m::inner(Input(0));
//> 1: test::m::take_then_imm(Input(0), Result(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, `&Inner` then the object input by value
//> 0: test::m::inner(Input(0));
//> 1: test::m::imm_then_take(Result(0), Input(0));

//# programmable --sender A
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, object result by value then its `&mut Inner`
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::take_then_mut(Result(0), Result(1));

//# programmable --sender A --inputs object(2,0)
// VALID: the object input by value with a reference into a different object
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::take_then_mut(Input(0), Result(1));
//> 3: test::m::delete(Result(0));
