// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses Test=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 2

//# publish --sender A
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
        let c2 = coin::mint(&mut treasury_cap, 2, ctx);
        let c3 = coin::mint(&mut treasury_cap, 300, ctx);
        let c4 = coin::mint(&mut treasury_cap, 400, ctx);
        let c5 = coin::mint(&mut treasury_cap, 500, ctx);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_transfer(c1, tx_context::sender(ctx));
        transfer::public_transfer(c2, tx_context::sender(ctx));
        transfer::public_transfer(c3, tx_context::sender(ctx));
        transfer::public_transfer(c4, tx_context::sender(ctx));
        transfer::public_transfer(c5, tx_context::sender(ctx));
    }
}

//# view-object 1,1

//# view-object 1,2

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

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{A}"]
}


//# transfer-object 1,1 --sender A --recipient B

//# transfer-object 1,2 --sender A --recipient B

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{A}", "0x2::sui::SUI"]
}

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


//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["@{B}", "invalid_coin_type"]
}

//# run-jsonrpc
{
  "method": "suix_getBalance",
  "params": ["invalid_address"]
}

