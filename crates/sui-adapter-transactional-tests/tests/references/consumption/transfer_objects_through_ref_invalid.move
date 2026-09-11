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

//# programmable --sender A --inputs @A
// INVALID: InvalidTransferObject at arg 0 of command 2, `&mut Obj`
//> 0: test::m::new();
//> 1: test::m::id_mut<test::m::Obj>(Result(0));
//> 2: TransferObjects([Result(1)], Input(0));
//> 3: test::m::delete(Result(0));

//# programmable --sender A --inputs @A
// INVALID: InvalidTransferObject at arg 0 of command 2, `&Obj`
//> 0: test::m::new();
//> 1: test::m::id<test::m::Obj>(Result(0));
//> 2: TransferObjects([Result(1)], Input(0));
//> 3: test::m::delete(Result(0));

//# programmable --sender A --inputs @A
// INVALID: InvalidTransferObject at arg 1 of command 3, a reference among values
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::id_mut<test::m::Obj>(Result(1));
//> 3: TransferObjects([Result(0), Result(2)], Input(0));
//> 4: test::m::delete(Result(1));
