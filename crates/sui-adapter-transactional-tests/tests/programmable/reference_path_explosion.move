// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m {
    public fun p16(
        x0: &u64, x1: &u64, x2: &u64, x3: &u64,
        x4: &u64, x5: &u64, x6: &u64, x7: &u64,
        x8: &u64, x9: &u64, x10: &u64, x11: &u64,
        x12: &u64, x13: &u64, x14: &u64, x15: &u64,
    ): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64) {
        (x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11, x12, x13, x14, x15)
    }
}

// Every call takes all 16 reference results from the preceding call and returns 16 references.
// The regex borrow graph must preserve all possible paths through each call, while the set-based
// verifier represents each result as a node with 16 parents.
//# programmable --inputs 0u64 0u64 0u64 0u64 0u64 0u64 0u64 0u64 0u64 0u64 0u64 0u64 0u64 0u64 0u64 0u64
//> 0: test::m::p16(Input(0), Input(1), Input(2), Input(3), Input(4), Input(5), Input(6), Input(7), Input(8), Input(9), Input(10), Input(11), Input(12), Input(13), Input(14), Input(15));
//> 1: test::m::p16(NestedResult(0,0), NestedResult(0,1), NestedResult(0,2), NestedResult(0,3), NestedResult(0,4), NestedResult(0,5), NestedResult(0,6), NestedResult(0,7), NestedResult(0,8), NestedResult(0,9), NestedResult(0,10), NestedResult(0,11), NestedResult(0,12), NestedResult(0,13), NestedResult(0,14), NestedResult(0,15));
//> 2: test::m::p16(NestedResult(1,0), NestedResult(1,1), NestedResult(1,2), NestedResult(1,3), NestedResult(1,4), NestedResult(1,5), NestedResult(1,6), NestedResult(1,7), NestedResult(1,8), NestedResult(1,9), NestedResult(1,10), NestedResult(1,11), NestedResult(1,12), NestedResult(1,13), NestedResult(1,14), NestedResult(1,15));
//> 3: test::m::p16(NestedResult(2,0), NestedResult(2,1), NestedResult(2,2), NestedResult(2,3), NestedResult(2,4), NestedResult(2,5), NestedResult(2,6), NestedResult(2,7), NestedResult(2,8), NestedResult(2,9), NestedResult(2,10), NestedResult(2,11), NestedResult(2,12), NestedResult(2,13), NestedResult(2,14), NestedResult(2,15));
