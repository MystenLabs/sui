// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test1=0x0 test2=0x1 --accounts A

// Simple publish test

// Passing

//# publish --dry-run

module test1::m1 {
    fun init(_: &sui::tx_context::TxContext) {
    }
}

//# publish

module test1::m2 {
    fun init(_: &sui::tx_context::TxContext) {
    }
}

// Failing

//# publish --dry-run

module test2::m3 {
    fun init(_: &sui::tx_context::TxContext) {
    }
}

//# publish

module test2::m4 {
    fun init(_: &sui::tx_context::TxContext) {
    }
}
