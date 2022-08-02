// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, wrong characteristic type name

//# init --addresses test=0x0

//# publish
module test::m {

    struct CharType has drop { }

    fun init(_: CharType, _ctx: &mut sui::tx_context::TxContext) {
    }
}
