// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Parent has key, store { id: UID }
public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun new_parent(ctx: &mut TxContext): Parent { Parent { id: object::new(ctx) } }
public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}

public fun uid_mut(p: &mut Parent): &mut UID { &mut p.id }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }

public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs @A
//> 0: test::m::new_parent();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0) 1
// SETUP: a fresh object added as a dynamic object field through the `&mut UID` reference
//> 0: test::m::uid_mut(Input(0));
//> 1: test::m::new();
//> 2: sui::dynamic_object_field::add<u64, test::m::Obj>(Result(0), Input(1), Result(1));

//# programmable --sender A --inputs object(2,0) 1 @A
// INVALID: InvalidReferenceArgument at arg 0 of command 3, remove while the child chain is live
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::dynamic_object_field::borrow_mut<u64, test::m::Obj>(Result(0), Input(1));
//> 2: test::m::inner_mut(Result(1));
//> 3: sui::dynamic_object_field::remove<u64, test::m::Obj>(Result(0), Input(1));
//> 4: test::m::use_mut<test::m::Inner>(Result(2));
//> 5: TransferObjects([Result(3)], Input(2));
