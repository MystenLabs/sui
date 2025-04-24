// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Transfer an object from A to B and verify that the coin is now included in the result of suix_getCoins
// for B and not included in the result for A.

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

//# transfer-object 1,0 --sender A --recipient B

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{A}"]
}

//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{B}"]
}
