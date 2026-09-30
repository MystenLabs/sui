// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoCopy has drop { value: u64 }

public fun no_copy(value: u64): NoCopy { NoCopy { value } }
public fun value_ref(n: &NoCopy): &u64 { &n.value }
public fun read_then_take(x: u64, n: NoCopy) { assert!(x == n.value, 0) }


//# programmable --inputs 1
// VALID: the read is the reference's last use and releases it before the root is moved
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::read_then_take(Result(1), Result(0));
