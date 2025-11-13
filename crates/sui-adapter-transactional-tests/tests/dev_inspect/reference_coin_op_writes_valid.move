// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests valid writes of mut references using coin operations

//# init --addresses test=0x0 --accounts A --allow-references-in-ptbs

//# publish
module test::m {

    public fun borrow_mut(
        c: &mut sui::coin::Coin<sui::sui::SUI>,
    ): &mut sui::coin::Coin<sui::sui::SUI> {
        c
    }

    public fun new_mut(): &mut sui::coin::Coin<sui::sui::SUI> {
        abort 0
    }

}

//# programmable --sender A --inputs 0 @A
// generate some coins for testing
//> SplitCoins(Gas, [Input(0), Input(0), Input(0)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1), NestedResult(0,2)], Input(1))

//# programmable --sender A --inputs 0 @A
// Can write to same coin ref via split coins
//> 0: sui::coin::zero<sui::sui::SUI>();
//> 1: test::m::borrow_mut(Result(0));
//> 2: SplitCoins(Result(1), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(1));
//> 4: test::m::borrow_mut(Result(1));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 0 @A object(2,0)
// Can write to same coin ref via Merge coins
//> 0: sui::coin::zero<sui::sui::SUI>();
//> 1: test::m::borrow_mut(Result(0));
//> 2: MergeCoins(Result(1), [Input(2)]);
//> 3: test::m::borrow_mut(Result(1));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 0 @A
// Can write to same coin via split coins
//> 0: sui::coin::zero<sui::sui::SUI>();
//> 1: test::m::borrow_mut(Result(0));
//> 2: SplitCoins(Result(0), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(1));
//> 4: test::m::borrow_mut(Result(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 0 @A object(2,1)
// Can write to same coin via Merge coins
//> 0: sui::coin::zero<sui::sui::SUI>();
//> 1: test::m::borrow_mut(Result(0));
//> 2: MergeCoins(Result(0), [Input(2)]);
//> 3: test::m::borrow_mut(Result(0));
//> TransferObjects([Result(0)], Input(1))
