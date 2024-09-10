// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various valid operations involving SplitCoins


//# init --addresses test=0x0 --accounts A B

//# publish
module test::m1 {
    use sui::coin;

    public fun ret_one_amount(): u64 {
        100
    }

    public fun transfer_(v: vector<coin::Coin<sui::sui::SUI>>, r: address) {
        v.do!(|c| {
            transfer::public_transfer(c, r);
        });
    }
}

//# programmable --sender A --inputs 100000 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// let's get ourselves a coin worth 1000
//# run sui::pay::split_and_transfer --type-args sui::sui::SUI --args object(2,0) 1000 @A --sender A

//# view-object 3,0

// split 100 off the freshly created coin
//# programmable --sender A --inputs object(3,0) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([NestedResult(0,0)], Input(2));

//# view-object 3,0

//# view-object 5,0


// split 100 off the freshly created coin twice
//# programmable --sender A --inputs object(3,0) 100 @B
//> 0: SplitCoins(Input(0), [Input(1), Input(1)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(2));

//# view-object 3,0

//# view-object 8,0

//# view-object 8,1

// split 100 off the freshly created coin twice taking one input from Move call
//# programmable --sender A --inputs object(3,0) 100 @B
//> 0: test::m1::ret_one_amount();
//> 1: SplitCoins(Input(0), [Result(0), Input(1)]);
//> TransferObjects([NestedResult(1,0), NestedResult(1,1)], Input(2));

//# view-object 3,0

//# view-object 12,0

//# view-object 12,1

// split 100 off the freshly created coin twice taking one input from Move call and transfer them
// using another Move call
//# programmable --sender A --inputs object(3,0) 100 @B
//> 0: test::m1::ret_one_amount();
//> 1: SplitCoins(Input(0), [Result(0), Input(1)]);
//> 2: MakeMoveVec<sui::coin::Coin<sui::sui::SUI>>([NestedResult(1,0), NestedResult(1,1)]);
//> test::m1::transfer_(Result(2), Input(2));

//# view-object 3,0

//# view-object 16,0

//# view-object 16,1
