// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various mismatched coin types for merge coins

//# init --addresses test=0x0 --accounts A

//# publish --sender A
module test::fake {
    use std::option;
    use sui::coin;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(witness, 2, b"FAKE", b"", b"", option::none(), ctx);
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
    }

}

//# programmable --sender A --inputs object(107) 100 @A
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# view-object 109

//# programmable --sender A --inputs object(107) 100
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs object(109)
//> MergeCoins(Gas, [Input(0)])

//# programmable --sender A --inputs object(107) 100 object(109) object(103)
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> MergeCoins(Result(0), [Input(2), Input(3)])

//# programmable --sender A --inputs object(107) 100
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> 1: SplitCoins(Result(0), [Input(1)]);
//> MergeCoins(Gas, [Result(1)])
