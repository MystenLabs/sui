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
        let c2 = coin::mint(&mut treasury_cap, 20, ctx);
        let c3 = coin::mint(&mut treasury_cap, 3000, ctx);
        let c4 = coin::mint(&mut treasury_cap, 4000, ctx);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_transfer(c1, tx_context::sender(ctx));
        transfer::public_transfer(c2, tx_context::sender(ctx));
        transfer::public_transfer(c3, tx_context::sender(ctx));
        transfer::public_transfer(c4, tx_context::sender(ctx));
    }
}

module Test::real {
    use sui::coin;

    public struct REAL has drop {}

    fun init(witness: REAL, ctx: &mut TxContext){
        let (mut treasury_cap, metadata) = coin::create_currency(
            witness,
            2,
            b"REAL",
            b"",
            b"",
            option::none(),
            ctx,
        );

        let c5 = coin::mint(&mut treasury_cap, 42, ctx);
        let c6 = coin::mint(&mut treasury_cap, 3700, ctx);
        let c7 = coin::mint(&mut treasury_cap, 1000000, ctx);
        let c8 = coin::mint(&mut treasury_cap, 25000000, ctx);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_transfer(c5, tx_context::sender(ctx));
        transfer::public_transfer(c6, tx_context::sender(ctx));
        transfer::public_transfer(c7, tx_context::sender(ctx));
        transfer::public_transfer(c8, tx_context::sender(ctx));
    }
}

//# transfer-object 1,2 --sender A --recipient B

//# transfer-object 1,1 --sender A --recipient B

//# transfer-object 1,6 --sender A --recipient B

//# programmable --sender A --inputs 500 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getAllBalances",
  "params": ["@{A}"]
}

//# run-jsonrpc
{
  "method": "suix_getAllBalances",
  "params": ["@{B}"]
}

//# transfer-object 1,6 --sender B --recipient A

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getAllBalances",
  "params": ["@{A}"]
}

//# run-jsonrpc
{
  "method": "suix_getAllBalances",
  "params": ["@{B}"]
}
