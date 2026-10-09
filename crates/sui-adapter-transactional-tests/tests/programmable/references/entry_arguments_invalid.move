// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A reference inherits the taint of its source: a hot potato outstanding on the root, or a shared
// object consumed into the root's vector, keeps the reference and any read of it out of private
// entry functions.

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
public fun heat(_: &Obj): Hot { Hot {} }
public fun cool(h: Hot) { let Hot {} = h; }
entry fun play(_: &Inner) {}
entry fun play_val(_: Inner) {}
entry fun play_obj(_: &mut Obj) {}
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
// INVALID: InvalidArgumentToPrivateEntryFunction at arg 0 of command 2, the reference's source holds a hot potato
//> 0: test::m::heat(Input(0));
//> 1: test::m::inner(Input(0));
//> 2: test::m::play(Result(1));
//> 3: test::m::cool(Result(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: InvalidArgumentToPrivateEntryFunction at arg 0 of command 2, a by-value read of a reference whose source holds a hot potato
//> 0: test::m::heat(Input(0));
//> 1: test::m::inner(Input(0));
//> 2: test::m::play_val(Result(1));
//> 3: test::m::cool(Result(0));

//# programmable --sender A --inputs object(3,0) 0
// INVALID: InvalidArgumentToPrivateEntryFunction at arg 0 of command 2, a reference into a vector holding a shared object
//> 0: MakeMoveVec([Input(0)]);
//> 1: std::vector::borrow_mut<test::m::Obj>(Result(0), Input(1));
//> 2: test::m::play_obj(Result(1));
//> 3: test::m::delete_all(Result(0));
