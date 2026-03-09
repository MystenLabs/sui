// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test is to verify that the coin balance derived from coins accurately reflects the summed
// value of the underlying coins. Create three coins and call get_balance on the Test::fake coin
// type, expecting 600. After merging the coins, get_balance should return the same balance of 600.

//# init --protocol-version 108 --addresses Test=0x0 --accounts A B --simulator

//# publish --sender A
module Test::fake {
    use sui::coin_registry;

    public struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (init, mut treasury_cap) = coin_registry::new_currency_with_otw(
            witness,
            2,
            b"FAKE".to_string(),
            b"Fake".to_string(),
            b"A fake coin for test purposes".to_string(),
            b"https://example.com/fake.png".to_string(),
            ctx,
        );

        let c1 = treasury_cap.mint(100, ctx);
        let c2 = treasury_cap.mint(200, ctx);
        let c3 = treasury_cap.mint(300, ctx);
        let metadata_cap = init.finalize(ctx);

        transfer::public_transfer(c1, ctx.sender());
        transfer::public_transfer(c2, ctx.sender());
        transfer::public_transfer(c3, ctx.sender());

        transfer::public_transfer(treasury_cap, ctx.sender());
        transfer::public_transfer(metadata_cap, @0x0);
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
