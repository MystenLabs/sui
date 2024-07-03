// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;

    public struct CoolMarker has key, store { id: UID }

    public entry fun purchase(coin: Coin<SUI>, ctx: &mut TxContext) {
        transfer::public_transfer(purchase_(coin, ctx), tx_context::sender(ctx))
    }

    public fun purchase_(coin: Coin<SUI>, ctx: &mut TxContext): CoolMarker {
        assert!(coin::value(&coin) >= 100, 0);
        transfer::public_transfer(coin, @0x70DD);
        CoolMarker { id: object::new(ctx) }
    }

}

// call an entry function
//# programmable --sender A --inputs 100  --gas-budget 10000000000

//> 0: SplitCoins(Gas, [Input(0)]); // split the coin as a limit
//> 1: test::m1::purchase(Result(0));

//# view-object 2,0

//# view-object 2,1

// call a non-entry function, but forget the object
//# programmable --sender A --inputs 100  --gas-budget 10000000000

//> 0: SplitCoins(Gas, [Input(0)]); /* split the coin as a limit */
//> 1: test::m1::purchase_(Result(0));

// call a non-entry function, and transfer the object
//# programmable --sender A --inputs 100 @A  --gas-budget 10000000000

//> 0: SplitCoins(Gas, [Input(0), Input(0)]); /* /* nested */*/
//> 1: test::m1::purchase_(NestedResult(0, 0));
//> 2: test::m1::purchase_(NestedResult(0, 1));
//> TransferObjects([Result(1), Result(2)], Input(1));

//# view-object 6,0

//# view-object 6,1

//# view-object 6,2

//# view-object 6,3
