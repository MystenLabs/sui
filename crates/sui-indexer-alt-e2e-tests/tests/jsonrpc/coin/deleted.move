// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test is to verify that deleted coins are not included in the result of suix_getCoins.
// We create two coins, of balances 12 and 34, call the rpc method to see both of them in the results.
// Then we merge the coins and call the rpc method again to see that only the merged coin with
// balance 46 is in the results.

//# init --protocol-version 70 --addresses Test=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 2

//# programmable --sender A --inputs 12 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 34 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# view-object 1,0

//# view-object 2,0

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{A}"]
}

//# programmable --sender A --inputs object(1,0) object(2,0)
//> 0: MergeCoins(Input(0), [Input(1)]);

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{A}"]
}
