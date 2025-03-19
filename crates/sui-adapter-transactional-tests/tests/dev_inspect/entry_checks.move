// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// entry checks disabled during dev inspect

//# init --addresses test=0x0 --accounts A

//# publish

module test::m {
    public fun return_u64(): u64 {
        0
    }
    entry fun entry_take_u64(_n: u64) {
    }
}

//# programmable --sender A --dev-inspect
//> 0: test::m::return_u64();
//> 1: test::m::entry_take_u64(Result(0));
