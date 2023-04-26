// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// init with entry is no longer allowed

//# init --addresses test=0x0

//# publish
module test::m {
    use sui::tx_context::TxContext;
    entry fun init(_: &mut TxContext) {
    }
}

//# run test::m::init
