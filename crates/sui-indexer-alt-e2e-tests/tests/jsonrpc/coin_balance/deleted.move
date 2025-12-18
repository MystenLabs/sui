// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test is to verify that the coin balance derived from coins accurately reflects the summed
// value of the underlying coins. Create three coins and call get_balance on the Test::fake coin
// type, expecting 600. After merging the coins, get_balance should return the same balance of 600.

//# init --protocol-version 70 --addresses Test=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 2

//# publish --sender A
#[allow(deprecated_usage)]
module Test::fake {
    use sui::coin;

    public struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (mut treasury_cap, metadata) = coin::create_currency(
            witness,
            2,
            b"FAKE",
            b"",
            b"",
            option::none(),
            ctx,
        );

        let c1 = coin::mint(&mut treasury_cap, 100, ctx);
        let c2 = coin::mint(&mut treasury_cap, 200, ctx);
        let c3 = coin::mint(&mut treasury_cap, 300, ctx);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_transfer(c1, tx_context::sender(ctx));
        transfer::public_transfer(c2, tx_context::sender(ctx));
        transfer::public_transfer(c3, tx_context::sender(ctx));
    }
}

//# create-checkpoint

//# view-object 1,1

//# view-object 1,2

//# view-object 1,3

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{A}", "@{Test}::fake::FAKE"]
}

//# programmable --sender A --inputs object(1,1) object(1,2) object(1,3)
//> 0: MergeCoins(Input(0), [Input(1), Input(2)]);

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{A}", "@{Test}::fake::FAKE"]
}
