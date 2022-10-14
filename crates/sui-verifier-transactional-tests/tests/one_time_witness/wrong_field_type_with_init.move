// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, wrong struct field type

//# init --addresses test=0x0

//# publish
module test::m {

    struct M has drop { value: u64 }

    fun init(_: M, _ctx: &mut sui::tx_context::TxContext) {
    }
}
