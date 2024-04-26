// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// invalid, the adapter should yell that the invoked functions have the wrong visibility

//# init --addresses Test=0x0

//# publish
module Test::M {
    public(package) fun t2(_: &mut TxContext) {
        abort 0
    }

    fun t3(_: &mut TxContext) {
        abort 0
    }

    public fun t4(x: &u64, _: &mut TxContext): &u64 {
        x
    }

    public fun t5(x: &mut u64, _: &mut TxContext): &mut u64 {
        x
    }

}

//# run Test::M::t2

//# run Test::M::t3

//# run Test::M::t4 --args 0

//# run Test::M::t5 --args 0
