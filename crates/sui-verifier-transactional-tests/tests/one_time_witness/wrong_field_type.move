// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// correct, wrong struct field type but not one-time witness candidate

//# init --addresses test=0x0

//# publish
module test::m {

    struct M has drop { value: u64 }

    fun init(_ctx: &mut sui::tx_context::TxContext) {
    }

    fun foo() {
        M { value: 7 };
        M { value: 42 };
    }
}
