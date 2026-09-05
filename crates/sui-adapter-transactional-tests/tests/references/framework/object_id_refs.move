// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID }

public fun new(ctx: &mut TxContext): Obj { Obj { id: object::new(ctx) } }
public fun check_id(id: &ID, o: &Obj) { assert!(*id == object::id(o), 0) }
public fun delete(o: Obj) { let Obj { id } = o; object::delete(id) }

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0)
// VALID: `&ID` from `borrow_id`, consumed by `&ID` parameters, then the object read again
//> 0: sui::object::borrow_id<test::m::Obj>(Input(0));
//> 1: sui::object::id_to_address(Result(0));
//> 2: test::m::check_id(Result(0), Input(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, the object deleted while its id is borrowed
//> 0: sui::object::borrow_id<test::m::Obj>(Input(0));
//> 1: test::m::delete(Input(0));
//> 2: sui::object::id_to_address(Result(0));
