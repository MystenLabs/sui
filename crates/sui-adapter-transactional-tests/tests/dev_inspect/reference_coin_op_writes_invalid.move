// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests invalid writes of mut references using coin operations

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

//# programmable --inputs 10 @A
// generate some coins for testing
//> SplitCoins(Gas, [Input(0), Input(0), Input(0)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1), NestedResult(0,2)], Input(1))

//# programmable --sender A --inputs 10 @A
// Cannot write to borrowed gas coin via split coins
//> 0: test::m::borrow_mut(Gas);
//> 1: SplitCoins(Gas, [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));
//> 3: test::m::borrow_mut(Result(0));

//# programmable --sender A --inputs 10 @A object(2,0)
// Cannot write to borrowed gas coin via Merge coins
//> 0: test::m::borrow_mut(Gas);
//> 1: MergeCoins(Gas, [Input(2)]);
//> 2: test::m::borrow_mut(Result(0));

//# programmable --sender A --inputs 10 @A object(2,0)
// Cannot write to borrowed coin via split coins
//> 0: test::m::borrow_mut(Input(2));
//> 1: SplitCoins(Input(2), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));
//> 3: test::m::borrow_mut(Result(0));

//# programmable --sender A --inputs 10 @A object(2,0) object(2,1)
// Cannot write to borrowed coin via Merge coins
//> 0: test::m::borrow_mut(Input(2));
//> 1: MergeCoins(Input(2), [Input(3)]);
//> 2: test::m::borrow_mut(Result(0));

//# programmable --sender A --inputs 10 @A
// Cannot write to borrowed fresh coin via split coins
//> 0: sui::coin::zero<sui::sui::SUI>();
//> 1: test::m::borrow_mut(Result(0));
//> 2: SplitCoins(Result(0), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(1));
//> 4: test::m::borrow_mut(Result(1));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 10 @A object(2,0)
// Cannot write to borrowed fresh coin via Merge coins
//> 0: sui::coin::zero<sui::sui::SUI>();
//> 1: test::m::borrow_mut(Result(0));
//> 2: MergeCoins(Result(0), [Input(2)]);
//> 3: test::m::borrow_mut(Result(1));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 10 @A
// Cannot write to borrowed coin via split coins
//> 0: test::m::new_mut();
//> 1: test::m::borrow_mut(Result(0));
//> 2: SplitCoins(Result(0), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(1));
//> 4: test::m::borrow_mut(Result(1));

//# programmable --sender A --inputs 10 @A object(2,0)
// Cannot write to borrowed coin via Merge coins
//> 0: test::m::new_mut();
//> 1: test::m::borrow_mut(Result(0));
//> 2: MergeCoins(Result(0), [Input(2)]);
//> 3: test::m::borrow_mut(Result(1));
