// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test basic coin transfer

//# init --addresses test=0x0 --accounts A B C

//# publish

/// gas heavy function
module test::m {

    use sui::dynamic_field::add;

    public struct Obj has key {
        id: object::UID,
        contents: vector<u8>

    }

    public entry fun large_vector(n: u64, ctx: &mut TxContext) {
        let mut v: vector<u64> = vector::empty();
        let mut i = 0;
        while (i < n) {
            vector::push_back(&mut v, i);
            i = i + 1;
        };

        transfer::transfer(Obj { id: object::new(ctx), contents: vector::empty() }, tx_context::sender(ctx))
    }
}

// Move all A coins to B
//# programmable --sender A --inputs @B
//> TransferObjects([Gas], Input(0))

// Return a small amount of coin to A
//# programmable --sender B --inputs 1 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// Account A now has 3,0
//# view-object 3,0

// not enough gas
//# programmable --sender A --inputs 100 --dry-run --gas-payment 3,0
//> 0: test::m::large_vector(Input(0));

// give A 5000000000 gas but split across 2 objects
//# programmable --sender B --inputs 250000 240000 @A
//> SplitCoins(Gas, [Input(0), Input(0)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(2))


// Account A now has 6,0 6,1
//# programmable --sender A --inputs 100 --dry-run --gas-payment 6,0 --gas-payment 6,1
//> 0: test::m::large_vector(Input(0));


// Account A now has 6,0 6,1
//# programmable --sender A --inputs 100 --dev-inspect --gas-payment 6,0 --gas-payment 6,1
//> 0: test::m::large_vector(Input(0));
