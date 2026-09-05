// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun write(r: &mut u8, v: u8) { *r = v }
public fun check(r: &u8, v: u8) { assert!(*r == v, 0) }
public fun check_bool(r: &bool, v: bool) { assert!(*r == v, 0) }
public fun take(_: u8) {}
public fun two<A, B>(_: &mut A, _: &mut B) {}
public fun use_mut<T>(_: &mut T) {}

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

//# programmable --inputs 0u8
// VALID: one input at two types in one call
//> 0: test::m::two<u8, bool>(Input(0), Input(0));

//# programmable --inputs 0u8
// INVALID: InvalidReferenceArgument at arg 0 of command 0, one input at one type twice
//> 0: test::m::two<u8, u8>(Input(0), Input(0));

//# programmable --inputs 0u8 7u8
// VALID: a `&mut bool` view live while the u8 view is borrowed mutably
//> 0: test::m::id_mut<bool>(Input(0));
//> 1: test::m::id_mut<u8>(Input(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: test::m::use_mut<bool>(Result(0));
