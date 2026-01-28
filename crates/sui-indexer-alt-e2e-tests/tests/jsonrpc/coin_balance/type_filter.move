// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --addresses Test=0x0 --accounts A B --simulator

//# publish --sender A
#[allow(deprecated_usage)]
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
        let c2 = treasury_cap.mint(2, ctx);
        let c3 = treasury_cap.mint(3000, ctx);
        let c4 = treasury_cap.mint(4000, ctx);
        let c5 = treasury_cap.mint(5000, ctx);
        let metadata_cap = init.finalize(ctx);


        transfer::public_transfer(c1, ctx.sender());
        transfer::public_transfer(c2, ctx.sender());
        transfer::public_transfer(c3, ctx.sender());
        transfer::public_transfer(c4, ctx.sender());
        transfer::public_transfer(c5, ctx.sender());

        transfer::public_transfer(treasury_cap, ctx.sender());
        transfer::public_transfer(metadata_cap, @0x0);
    }
}

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{A}", "@{Test}::fake::FAKE"]
}

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{A}"]
}


//# run-jsonrpc
{
  "method": "suix_getAllBalances",
  "params": ["@{A}"]
}
