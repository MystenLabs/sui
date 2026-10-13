// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Reference results as arguments to private entry functions. The private-entry check sees the
// new argument forms: a reference passed by reference, and a reference read by value.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct Hot {}

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun share(ctx: &mut TxContext) {
    transfer::public_share_object(Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } })
}
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun heat(_: &Obj): Hot { Hot {} }
public fun cool(h: Hot) { let Hot {} = h; }
public fun use_mut<T>(_: &mut T) {}
entry fun play(_: &Inner) {}
entry fun play_mut(_: &mut Inner) {}
entry fun play_val(_: Inner) {}
entry fun play_obj(_: &mut Obj) {}
public entry fun public_play(_: &Inner) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }
public fun delete_all(mut v: vector<Obj>) {
    while (!v.is_empty()) { delete(v.pop_back()) };
    v.destroy_empty()
}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A
//> 0: test::m::share();

//# programmable --sender A --inputs object(2,0)
// VALID: reference results rooted in an input reach a private entry function by `&` and `&mut`
//> 0: test::m::inner(Input(0));
//> 1: test::m::play(Result(0));
//> 2: test::m::inner_mut(Input(0));
//> 3: test::m::play_mut(Result(2));

//# programmable --sender A --inputs object(2,0)
// VALID: a reference whose source holds a hot potato can reach a public entry function
//> 0: test::m::heat(Input(0));
//> 1: test::m::inner(Input(0));
//> 2: test::m::public_play(Result(1));
//> 3: test::m::cool(Result(0));

//# programmable --sender A --inputs object(2,0)
// VALID: the hot potato is cooled before the reference reaches the private entry function
//> 0: test::m::heat(Input(0));
//> 1: test::m::cool(Result(0));
//> 2: test::m::inner(Input(0));
//> 3: test::m::play(Result(2));

//# programmable --sender A --inputs object(2,0)
// VALID: a by-value read of the reference reaches the private entry function once the hot potato is cooled
//> 0: test::m::heat(Input(0));
//> 1: test::m::cool(Result(0));
//> 2: test::m::inner(Input(0));
//> 3: test::m::play_val(Result(2));

//# programmable --sender A --inputs 0
// VALID: a reference into a vector holding a fresh owned object reaches a private entry function
//> 0: test::m::new();
//> 1: MakeMoveVec([Result(0)]);
//> 2: std::vector::borrow_mut<test::m::Obj>(Result(1), Input(0));
//> 3: test::m::play_obj(Result(2));
//> 4: test::m::delete_all(Result(1));

//# programmable --sender A --inputs object(3,0) 0
// VALID: a reference into a vector holding a shared object reaches a public function
//> 0: MakeMoveVec([Input(0)]);
//> 1: std::vector::borrow_mut<test::m::Obj>(Result(0), Input(1));
//> 2: test::m::use_mut<test::m::Obj>(Result(1));
//> 3: test::m::delete_all(Result(0));
