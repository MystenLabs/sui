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

    entry fun get_objects(ctx: &mut TxContext) {
        let mut i = 0u64;
        while (i < 5) {
            let obj = Obj { id: object::new(ctx), contents: vector::empty() };
            transfer::transfer(obj, tx_context::sender(ctx));
            i = i + 1;
        };
    }

    entry fun destroy_object(obj: Obj) {
        let Obj { id, contents: _ } = obj;
        id.delete();
    }

    public entry fun large_storage_func(n: u64, ctx: &mut TxContext) {
        let mut v: vector<u64> = vector::empty();
        let mut i = 0u64;
        while (i < n) {
            vector::push_back(&mut v, i);
            i = i + 1;
        };

        transfer::transfer(Obj { id: object::new(ctx), contents: vector::empty() }, tx_context::sender(ctx))
    }

    // no objects just computation
    public entry fun large_compute_func(n: u64) {
        let mut v: vector<u64> = vector::empty();
        let mut i = 0u64;
        while (i < n) {
            vector::push_back(&mut v, i);
            i = i + 1;
        };
    }
}

// Give A a 5 objects 2,0 to 2,4
//# programmable --sender A
//> 0: test::m::get_objects();

// Move all A coins to B
//# programmable --sender A --inputs @B
//> TransferObjects([Gas], Input(0))

// Return a small amount of coin to A
//# programmable --sender B --inputs 2 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// Account A now has gas object 4,0 and a balance of 2
//# view-object 4,0

// Not enough gas for large_storage_func()
//# programmable --sender A --inputs 100 --dry-run --gas-payment 4,0
//> 0: test::m::large_storage_func(Input(0));

// Give A enough gas to send transaction after rebates, it should still fail
//# programmable --sender B --inputs 2499999999 @A
//> SplitCoins(Gas, [Input(0), Input(0)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(1))

// Account A now has 7,0 7,1 that are 2 gas short of the needed amount, note gas smashing rebate also occurs
//# programmable --sender A --inputs 100 --dry-run --gas-payment 7,0 --gas-payment 7,1
//> 0: test::m::large_storage_func(Input(0));

// Destroying an object before the transaction ends does not allow the transaction to succeed
//# programmable --sender A --inputs object(2,0) 100 --dry-run --gas-payment 7,0 --gas-payment 7,1
//> 0: test::m::destroy_object(Input(0));
//> 1: test::m::large_storage_func(Input(1));

// Include 3,0 in the gas payment, it should succeed
//# programmable --sender A --inputs 100 --dry-run --gas-payment 7,0 --gas-payment 7,1 --gas-payment 4,0
//> 0: test::m::large_storage_func(Input(0));

// Return the balance of A to zero
//# programmable --sender A --inputs object(7,0) --dry-run --gas-payment 4,0

// Return a small amount of coin to A
//# programmable --sender B --inputs 2 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// Give A enough gas to send transaction
//# programmable --sender B --inputs 2499999999 @A
//> SplitCoins(Gas, [Input(0), Input(0)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(1))

// Not enough gas for large_compute_func() when excluding 12,0, gas smashing rebates
//# programmable --sender A --inputs 100 --dry-run --gas-payment 13,0 --gas-payment 13,1
//> 0: test::m::large_compute_func(Input(0));

// Destroying an object before the transaction ends does not allow the transaction to succeed
//# programmable --sender A --inputs object(2,0) 100 --dry-run --gas-payment 13,0 --gas-payment 13,1
//> 0: test::m::destroy_object(Input(0));
//> 1: test::m::large_compute_func(Input(1));

// Include 3,0 in the gas payment, it should succeed
//# programmable --sender A --inputs 100 --dry-run --gas-payment 13,0 --gas-payment 13,1 --gas-payment 12,0
//> 0: test::m::large_compute_func(Input(0));
