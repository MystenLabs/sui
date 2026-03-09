// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Transfer a Test::fake coin from A to B and verify that the balance is reflected under B and not
// A.

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
        let c2 = treasury_cap.mint(100, ctx);
        let c3 = treasury_cap.mint(100, ctx);
        let metadata = init.finalize(ctx);

        transfer::public_transfer(c1, ctx.sender());
        transfer::public_transfer(c2, ctx.sender());
        transfer::public_transfer(c3, ctx.sender());

        transfer::public_transfer(treasury_cap, ctx.sender());
        transfer::public_transfer(metadata, @0x0);
    }
}

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{A}", "@{Test}::fake::FAKE"]
}

//# transfer-object 1,1 --sender A --recipient B

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{A}", "@{Test}::fake::FAKE"]
}

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{B}", "@{Test}::fake::FAKE"]
}


//# transfer-object 1,2 --sender A --recipient B

//# transfer-object 1,3 --sender A --recipient B

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{A}", "@{Test}::fake::FAKE"]
}

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{B}", "@{Test}::fake::FAKE"]
}
