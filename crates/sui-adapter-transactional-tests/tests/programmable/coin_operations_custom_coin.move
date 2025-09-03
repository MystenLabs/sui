// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests coin operations with custom coins

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

//# programmable --sender A --inputs object(1,2) 100 object(2,0) 1 @A
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> 1: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> 2: SplitCoins(Result(0), [Input(3)]);
//> 3: SplitCoins(Input(2), [Input(3)]);
//> MergeCoins(Result(1), [Result(0), Input(2), Result(2), Result(3)]);
//> TransferObjects([Result(1)], Input(4))
