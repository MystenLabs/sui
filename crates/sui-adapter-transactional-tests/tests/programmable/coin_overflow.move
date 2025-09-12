// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests coin overflow... which isn't actually possible without directly editing the coin bytes

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

//# programmable --sender A --inputs object(1,2) 18446744073709551614 @A
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(1,2) 1 @A
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(2,0)
//> MergeCoins(Input(0), [Input(0)]);
