// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoCopy has drop { value: u64 }
public struct Obj has key, store { id: UID, to: address }

public fun no_copy(value: u64): NoCopy { NoCopy { value } }
public fun value_ref(n: &NoCopy): &u64 { &n.value }
public fun read_then_take(x: u64, n: NoCopy) { assert!(x == n.value, 0) }
public fun read_then_mut(x: u64, n: &mut NoCopy) { n.value = x + 1 }
public fun mut_then_read(n: &mut NoCopy, x: u64) { n.value = x + 1 }
public fun new(to: address, ctx: &mut TxContext): Obj { Obj { id: object::new(ctx), to } }
public fun to(o: &Obj): &address { &o.to }

//# programmable --inputs 1
// VALID: the read is the reference's last use and releases it before the root is moved
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::read_then_take(Result(1), Result(0));

//# programmable --inputs 1
// VALID: read then a fresh `&mut` borrow of the root
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::read_then_mut(Result(1), Result(0));

//# programmable --inputs 1
// VALID: a fresh `&mut` borrow of the root then the read; transferability is checked after all arguments
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::mut_then_read(Result(0), Result(1));
