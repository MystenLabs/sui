// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This illustrates the "inconsistency" of suix_getCoins, as opposed to graphql's consistency feature.
// In addition to the gas coin, we create a coin with balance 3400 and another coin with balance 12000 at checkpoint 1.
// We query with limit 2 and get a cursor pointing to the 12000 coin.
// We update the 3400 coin's balance to 1400 at checkpoint 2.
// We query the coin using the cursor we got from the first query and see two coins, one with a balance of 1400.
// In graphql, the coin with 1400 would be from checkpoint_viewed_at = 1, and has balance 3400.

//# init --protocol-version 70 --addresses Test=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 2

//# programmable --sender A --inputs 12000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 3400 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# view-object 1,0

//# view-object 2,0

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, null, 2]
}

//# programmable --sender A --inputs object(2,0) 2000 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: TransferObjects([Result(0)], Input(2))

//# create-checkpoint

//# run-jsonrpc --cursors bcs(bin(0u8,@{A},0x2::coin::Coin<0x2::sui::SUI>,1u8,!12000,@{obj_1_0}))
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, "@{cursor_0}"]
}
