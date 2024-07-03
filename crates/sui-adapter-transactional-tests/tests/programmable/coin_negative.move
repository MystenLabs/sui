// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests coin balance going negative on split

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

//# programmable --sender A --inputs object(1,2) 1 @A
//> 0: sui::coin::mint<test::fake::FAKE>(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(2,0) 2
//> SplitCoins(Input(0), [Input(1)]);

//# programmable --sender A --inputs 18446744073709551615  --gas-budget 10000000000
//> SplitCoins(Gas, [Input(0)])
