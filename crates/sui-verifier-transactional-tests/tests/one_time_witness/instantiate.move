// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, struct type incorrectly instantiated

//# init --addresses test=0x0

//# publish
module test::m {

    struct M has drop { }

    fun init(_: M, _ctx: &mut sui::tx_context::TxContext) {
    }


    fun pack(): M {
        M {}
    }
}
