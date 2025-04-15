// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

// Transactions:
//
// 1. Create a new gas coin and transfer it.
// 2. Split off some more funds from that coin and transfer that too.
// 3. Merge the split coin back into the gas coin.
// 4. Merge the original coin back into the gas coin.
//
// RPC queries:
//
// 1. All Transactions
// 2. Transactions affecting coin 1 (there should be 3).
// 3. Transactions affecting coin 2 (there should be 2).
// 4. The first transaction affecting coin 2, iterating forwards.
// 5. The last transaction affecting coin 2, iterating backwards.
// 6. The last transaction affecting coin 1, iterating forwards.
// 7. The middle transaction affecting coin 1, iterating forwards.

//# programmable --sender A --inputs 42 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(1,0) 41 @A
//> SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(2,0)
//> MergeCoins(Gas, [Input(0)])

//# programmable --sender A --inputs object(1,0)
//> MergeCoins(Gas, [Input(0)])

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [{}]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "AffectedObject": "@{obj_1_0}" } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "AffectedObject": "@{obj_2_0}" } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "AffectedObject": "@{obj_2_0}" } },
    null, 1, false
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "AffectedObject": "@{obj_2_0}" } },
    null, 1, true
  ]
}

//# run-jsonrpc --cursors 2
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "AffectedObject": "@{obj_1_0}" } },
    "@{cursor_0}", 1, false
  ]
}

//# run-jsonrpc --cursors 1
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "AffectedObject": "@{obj_1_0}" } },
    "@{cursor_0}", 1, false
  ]
}
