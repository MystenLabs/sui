// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// attempt to return a reference from a function

//# init --addresses test=0x0 --accounts A

//# publish

module test::m {
    public fun return_ref(n: &u64): &u64 {
        n
    }
}

//# programmable --sender A --inputs 0 --dry-run
//> 0: test::m::return_ref(Input(0));
