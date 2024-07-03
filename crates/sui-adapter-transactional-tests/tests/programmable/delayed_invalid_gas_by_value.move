// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that gas-by-value rules come after taken/borrow rules

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public fun take<T: key>(_: T) { abort 0 }
    public fun imm<T: key>(_: &T, _: T) { abort 0 }
    public fun mut_<T: key>(_: &mut T, _: T) { abort 0 }
}

//# programmable --sender A --inputs @A
//> TransferObjects([Gas], Input(0));
//> test::m1::take<sui::coin::Coin<sui::sui::SUI>>(Gas)

//# programmable
//> test::m1::imm<sui::coin::Coin<sui::sui::SUI>>(Gas, Gas)

//# programmable
//> test::m1::mut_<sui::coin::Coin<sui::sui::SUI>>(Gas, Gas)
