// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoCopy has drop { value: u64 }

public fun no_copy(value: u64): NoCopy { NoCopy { value } }
public fun value_ref(n: &NoCopy): &u64 { &n.value }

public fun read_then_mut(x: u64, n: &mut NoCopy) { n.value = x + 1 }
public fun mut_then_read(n: &mut NoCopy, x: u64) { n.value = x + 1 }


//# programmable --inputs 1
// VALID: read then a fresh `&mut` borrow of the root
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::read_then_mut(Result(1), Result(0));

//# programmable --inputs 1
// VALID: a fresh mutable borrow and a last-use read of its child can be arguments in that order
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::mut_then_read(Result(0), Result(1));
