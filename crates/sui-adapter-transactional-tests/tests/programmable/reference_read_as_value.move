// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Using a returned reference in a non-reference position (where the underlying type matches)
// produces a `Read` argument internally.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m {
    public fun id_ref(x: &u64): &u64 {
        x
    }

    public fun check(value: u64, expected: u64) {
        assert!(value == expected, 0);
    }
}

//# programmable --sender A --inputs 112u64
//> 0: test::m::id_ref(Input(0));
//> 1: test::m::check(Result(0), Input(0));
