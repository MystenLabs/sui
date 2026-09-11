// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun take_check(x: u64, v: u64) { assert!(x == v, 0) }

//# programmable --inputs 0 7
// VALID: by reference and by value after the write
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::write(Result(0), Input(1));
//> 2: test::m::check(Input(0), Input(1));
//> 3: test::m::take_check(Input(0), Input(1));

//# programmable --inputs 0 7 8
// VALID: a second reference sees the first write and its own write is seen after
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::write(Result(0), Input(1));
//> 2: test::m::id_mut<u64>(Input(0));
//> 3: test::m::check(Result(2), Input(1));
//> 4: test::m::write(Result(2), Input(2));
//> 5: test::m::check(Input(0), Input(2));

//# programmable --inputs 0 7
// VALID: two by-value uses of the input both see the write
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::write(Result(0), Input(1));
//> 2: test::m::take_check(Input(0), Input(0));
