// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct S has copy { f: u64 }

public fun s(): S { S { f: 0 } }
public fun f_mut(x: &mut S): &mut u64 { &mut x.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun take(x: S) { let S { f: _ } = x; }
public fun take_check(x: S, expected: u64) {
    let S { f } = x;
    assert!(f == expected, 0)
}

//# programmable --inputs 7
// VALID: copying a borrowed non-drop value leaves the original available for consumption after the reference's last use
//> 0: test::m::s();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::take(Result(0));
//> 3: test::m::write(Result(1), Input(0));
//> 4: test::m::take_check(Result(0), Input(0));
