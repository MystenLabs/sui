// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoCopy has drop { v: u64 }
public fun nc(v: u64): NoCopy { NoCopy { v } }
public fun check_nc(r: &NoCopy, v: u64) { assert!(r.v == v, 0) }

//# programmable --inputs 1
// INVALID: CannotMoveBorrowedValue at arg 0 of command 3, a non-copy option consumed while borrowed
//> 0: test::m::nc(Input(0));
//> 1: std::option::some<test::m::NoCopy>(Result(0));
//> 2: std::option::borrow<test::m::NoCopy>(Result(1));
//> 3: std::option::destroy_some<test::m::NoCopy>(Result(1));
//> 4: test::m::check_nc(Result(2), Input(0));
