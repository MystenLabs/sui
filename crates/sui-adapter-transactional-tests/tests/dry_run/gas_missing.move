// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test basic coin transfer

//# init --addresses test=0x0 --accounts A B C

//# publish

/// gas heavy function
module test::m {


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
//# programmable --sender B --inputs 2 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// Account A now has 3,0
//# view-object 3,0

// Not enough gas for large_vector()
//# programmable --sender A --inputs 100 --dry-run --gas-payment 3,0
//> 0: test::m::large_vector(Input(0));

// Give A enough gas to send transaction after rebates, it should still fail
//# programmable --sender B --inputs 2499999999 @A
//> SplitCoins(Gas, [Input(0), Input(0)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(1))

// Account A now has 6,0 6,1 that are 2 gas short of the needed amount
//# programmable --sender A --inputs 100 --dry-run --gas-payment 6,0 --gas-payment 6,1
//> 0: test::m::large_vector(Input(0));

// Include 3,0 in the gas payment, it should succeed
//# programmable --sender A --inputs 100 --dry-run --gas-payment 6,0 --gas-payment 6,1 --gas-payment 3,0
//> 0: test::m::large_vector(Input(0));
