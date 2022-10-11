// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, wrong type of the init function's first param

//# init --addresses test=0x0

//# publish
module test::m {

    struct M has drop { }

    struct N has drop { }

    fun init(_: N, _ctx: &mut sui::tx_context::TxContext) {
    }
}
