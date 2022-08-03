// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, characteristic type candidate used in a different module

//# init --addresses test1=0x0 test2=0x0

//# publish
module test1::m {

    struct M has drop { }
}

//# publish
module test2::n {

    fun init(_: test1::m::M, _ctx: &mut sui::tx_context::TxContext) {
    }
}
