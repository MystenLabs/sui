// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, the adapter should yell that the invoked functions have the wrong visibility

//# init --addresses Test=0x0

//# publish
module Test::M {
    use sui::tx_context::TxContext;

    public fun t1(_: &mut TxContext) {
        abort 0
    }

    public(friend) fun t2(_: &mut TxContext) {
        abort 0
    }

    fun t3(_: &mut TxContext) {
        abort 0
    }

}

//# run Test::M::t1

//# run Test::M::t2

//# run Test::M::t3
