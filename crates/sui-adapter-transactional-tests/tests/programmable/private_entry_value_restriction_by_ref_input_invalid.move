// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that object values cannot be used private entry functions if they have been
// dirtied by non-entry functions

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    use sui::coin::Coin;
    use sui::sui::SUI;

    public struct R has key, store { id: UID }
    public fun r(ctx: &mut TxContext): R { R { id: object::new(ctx) } }

    public fun v(): u64 { 100 }

    public fun id(r: R): R { r }
    public fun dirty(_: &mut R) {}

    entry fun priv(_: R) { abort 0 }
    entry fun coin(_: &mut Coin<SUI>) {}
}

//# programmable --sender A --inputs @A
//> 0: test::m1::r();
//> TransferObjects([Result(0)], Input(0))


// cannot use results from other functions

//# programmable
//> 0: test::m1::r();
//> test::m1::priv(Result(0));

//# programmable --sender A --inputs object(2,0)
//> 0: test::m1::id(Input(0));
//> test::m1::priv(Result(0));

// cannot use an object once it has been used in a non-entry function

//# programmable --sender A --inputs object(2,0)
//> 0: test::m1::dirty(Input(0));
//> test::m1::priv(Input(0));

// the result of the function makes the split coin dirty

//# programmable --sender A --inputs @A  --gas-budget 10000000000
//> 0: test::m1::v();
//> 1: SplitCoins(Gas, [Result(0)]);
//> test::m1::coin(Gas);
//> TransferObjects([Result(1)], Input(0))

//# programmable --sender A --inputs @A  --gas-budget 10000000000
//> 0: test::m1::v();
//> 1: SplitCoins(Gas, [Result(0)]);
//> test::m1::coin(Result(1));
//> TransferObjects([Result(1)], Input(0))
