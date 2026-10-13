// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// t2 and t3 have the wrong visibility; t4 and t5 return references, which are allowed

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

// Used to be disallowed
//# run Test::M::t4 --args 0

// Used to be disallowed
//# run Test::M::t5 --args 0
