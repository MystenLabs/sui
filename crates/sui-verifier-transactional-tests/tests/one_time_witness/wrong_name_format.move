// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, wrong one-time witness type name format

//# init --addresses test=0x0

//# publish
module test::mod {

    struct Mod has drop { }

    fun init(_: Mod, _ctx: &mut sui::tx_context::TxContext) {
    }
}
