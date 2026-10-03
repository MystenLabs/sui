// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }

//# programmable --inputs 1
// VALID: a copyable option is copied, not moved, by `destroy_some` while borrowed
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow<u64>(Result(0));
//> 2: std::option::destroy_some<u64>(Result(0));
//> 3: test::m::check(Result(1), Input(0));
