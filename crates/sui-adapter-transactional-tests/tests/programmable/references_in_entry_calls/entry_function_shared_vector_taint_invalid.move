// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, f: u64 }

public fun share(ctx: &mut TxContext) {
    transfer::public_share_object(Obj { id: object::new(ctx), f: 0 })
}

entry fun play_mut(_: &mut Obj) {}
public fun delete(o: Obj) { let Obj { id, f: _ } = o; object::delete(id) }
public fun delete_all(mut v: vector<Obj>) {
    while (!v.is_empty()) { delete(v.pop_back()) };
    v.destroy_empty()
}

//# programmable --sender A
//> 0: test::m::share();

//# programmable --sender A --inputs object(2,0) 0
// INVALID: InvalidArgumentToPrivateEntryFunction at arg 0 of command 2, a reference into a vector containing a shared object cannot reach a private entry function
//> 0: MakeMoveVec([Input(0)]);
//> 1: std::vector::borrow_mut<test::m::Obj>(Result(0), Input(1));
//> 2: test::m::play_mut(Result(1));
//> 3: test::m::delete_all(Result(0));
