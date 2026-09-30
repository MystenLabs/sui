// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoCopy has drop { value: u64 }
public struct Obj has key, store { id: UID, to: address }

public fun no_copy(value: u64): NoCopy { NoCopy { value } }
public fun value_ref(n: &NoCopy): &u64 { &n.value }
public fun take_then_read(n: NoCopy, x: u64) { assert!(x == n.value, 0) }
public fun new(to: address, ctx: &mut TxContext): Obj { Obj { id: object::new(ctx), to } }
public fun to(o: &Obj): &address { &o.to }

//# programmable --inputs 1
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, the root is moved before the read releases the reference
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::take_then_read(Result(0), Result(1));

//# programmable --sender A --inputs @A
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, objects are processed before the recipient read
//> 0: test::m::new(Input(0));
//> 1: test::m::to(Result(0));
//> 2: TransferObjects([Result(0)], Result(1));
