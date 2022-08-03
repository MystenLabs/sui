// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, wrong characteristic type name format

//# init --addresses test=0x0

//# publish
module test::mod {

    struct Mod has drop { }

    fun init(_: Mod, _ctx: &mut sui::tx_context::TxContext) {
    }
}
