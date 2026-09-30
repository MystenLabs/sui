// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun write(r: &mut u8, v: u8) { *r = v }
public fun check(r: &u8, v: u8) { assert!(*r == v, 0) }
public fun check_bool(r: &bool, v: bool) { assert!(*r == v, 0) }

//# programmable --inputs 0u8 7u8
// VALID: write through `&mut u8`, visible to a later use of the same input
//> 0: test::m::id_mut<u8>(Input(0));
//> 1: test::m::write(Result(0), Input(1));
//> 2: test::m::check(Input(0), Input(1));

//# programmable --inputs 0u8 7u8 false
// VALID: the bool view of the input is a different location from the u8 view
//> 0: test::m::id_mut<u8>(Input(0));
//> 1: test::m::write(Result(0), Input(1));
//> 2: test::m::check_bool(Input(0), Input(2));
