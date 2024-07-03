// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// test invalid usages of coin commands

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    // not a native coin, but same type structure and BCS layout
    public struct Coin<phantom T> has key, store {
        id: UID,
        value: u64,
    }

    public fun mint<T>(ctx: &mut TxContext): Coin<T> {
        Coin {
            id: object::new(ctx),
            value: 1000000,
        }
    }

}

// split non-coin
//# programmable --sender A --inputs 0
//> 0: test::m1::mint<sui::sui::SUI>();
//> SplitCoins(Result(0), [Input(0)])

// merge into non-coin
//# programmable --sender A --inputs 0
//> 0: test::m1::mint<sui::sui::SUI>();
//> MergeCoins(Result(0), [Gas])

// merge non-coin into gas
//# programmable --sender A --inputs 0
//> 0: test::m1::mint<sui::sui::SUI>();
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 10000u64
//> MergeCoins(Gas, [Input(0)])

//# programmable --sender A --inputs 10000u64
//> MergeCoins(Gas, [Input(0)])
