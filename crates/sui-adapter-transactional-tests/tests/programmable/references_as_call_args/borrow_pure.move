// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun write(r: &mut u8, v: u8) { *r = v }

public fun two<A, B>(_: &mut A, _: &mut B) {}
public fun use_mut<T>(_: &mut T) {}

//# programmable --inputs 0u8
// VALID: one input at two types in one call
//> 0: test::m::two<u8, bool>(Input(0), Input(0));

//# programmable --inputs 0u8 7u8
// VALID: a `&mut bool` view live while the u8 view is borrowed mutably
//> 0: test::m::id_mut<bool>(Input(0));
//> 1: test::m::id_mut<u8>(Input(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: test::m::use_mut<bool>(Result(0));
