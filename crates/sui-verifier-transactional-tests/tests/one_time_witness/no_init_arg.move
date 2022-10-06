// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, no one-time witness type parameter in init

//# init --addresses test=0x0

//# publish
module test::m {

    struct M has drop { value: bool }

    fun init(_: &mut sui::tx_context::TxContext) {
    }
}
