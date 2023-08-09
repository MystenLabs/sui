// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests valid gas coin usage by reference

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public entry fun t1<T: key>(_: &T) {
    }
    public fun t2<T: key>(_: &T) {
    }
    entry fun t3<T: key>(_: &T) {
    }
    public entry fun t4<T: key>(_: &mut T) {
    }
    public fun t5<T: key>(_: &mut T) {
    }
    entry fun t6<T: key>(_: &mut T) {
    }
}

// can pass to Move function by ref
//# programmable --sender A
//> test::m1::t1<sui::coin::Coin<sui::sui::SUI>>(Gas)

//# programmable --sender A
//> test::m1::t2<sui::coin::Coin<sui::sui::SUI>>(Gas)

//# programmable --sender A
//> test::m1::t2<sui::coin::Coin<sui::sui::SUI>>(Gas)

//# programmable --sender A
//> test::m1::t4<sui::coin::Coin<sui::sui::SUI>>(Gas)

//# programmable --sender A
//> test::m1::t5<sui::coin::Coin<sui::sui::SUI>>(Gas)

//# programmable --sender A
//> test::m1::t6<sui::coin::Coin<sui::sui::SUI>>(Gas)

// can pass to merge and split
//# programmable --sender A --inputs 10  --gas-budget 10000000000
//> 0: SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])
