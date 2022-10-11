// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// correct, no field specified at source level

//# init --addresses test=0x0

//# publish
module test::m {

    struct M has drop { }

    fun init(_: M, _ctx: &mut sui::tx_context::TxContext) {
    }
}
