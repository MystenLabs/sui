// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, one-time witness type candidate used in a different module

//# init --addresses test1=0x0 test2=0x0

//# publish
module test1::m {

    struct M has drop { }
}

//# publish --dependencies test1
module test2::n {

    fun init(_: test1::m::M, _ctx: &mut sui::tx_context::TxContext) {
    }
}
