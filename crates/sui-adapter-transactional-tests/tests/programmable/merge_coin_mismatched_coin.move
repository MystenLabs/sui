// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various mismatched coin types for merge coins

//# init --addresses test=0x0 --accounts A

//# publish --sender A
module test::fake {
    use sui::coin;

    public struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(witness, 2, b"FAKE", b"", b"", option::none(), ctx);
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
    }

}

//# programmable --sender A --inputs object(1,2) 100 @A
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# view-object 2,0

//# programmable --sender A --inputs object(1,2) 100
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs object(2,0)
//> MergeCoins(Gas, [Input(0)])


//# programmable --sender A --inputs object(1,2) 100 object(2,0)
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> 1: SplitCoins(Gas, [Input(1)]);
//> MergeCoins(Result(0), [Input(2), Result(1)])

//# programmable --sender A --inputs object(1,2) 100
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> 1: SplitCoins(Result(0), [Input(1)]);
//> MergeCoins(Gas, [Result(1)])
