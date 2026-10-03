// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID }

public fun new(ctx: &mut TxContext): Obj { Obj { id: object::new(ctx) } }
public fun id<T>(t: &T): &T { t }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun delete(o: Obj) { let Obj { id } = o; object::delete(id) }
public fun delete_all(mut v: vector<Obj>) {
    while (!v.is_empty()) { delete(v.pop_back()) };
    v.destroy_empty()
}

//# programmable
// INVALID: InvalidMakeMoveVecNonObjectArgument at arg 0 of command 2, untyped with a reference first
//> 0: test::m::new();
//> 1: test::m::id_mut<test::m::Obj>(Result(0));
//> 2: MakeMoveVec([Result(1)]);
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: TypeMismatch at arg 1 of command 3, untyped with a reference after an object
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::id_mut<test::m::Obj>(Result(1));
//> 3: MakeMoveVec([Result(0), Result(2)]);
//> 4: test::m::delete_all(Result(3));
//> 5: test::m::delete(Result(1));

//# programmable
// INVALID: TypeMismatch at arg 0 of command 2, typed with a reference to a non-copy value
//> 0: test::m::new();
//> 1: test::m::id<test::m::Obj>(Result(0));
//> 2: MakeMoveVec<test::m::Obj>([Result(1)]);
//> 3: test::m::delete_all(Result(2));
//> 4: test::m::delete(Result(0));
