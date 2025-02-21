// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B C --simulator

// Transactions:
//
// 1. A sends to self
// 2. A sends to B
// 3. A sends to C
// 4. A sends to B and C
// 5. B sends to A
//
// RPC queries:
//
// 1. All Transactions
// 2, 3,  4. Transactions affecting A, B, C respectively.
// 5, 6,  7. Transactions sent by A, B, C respectively.
// 8. Transactions sent by A to B.

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 44 @C
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 45 @B @C
//> 0: SplitCoins(Gas, [Input(0), Input(0)]);
//> TransferObjects([NestedResult(0, 0)], Input(1));
//> TransferObjects([NestedResult(0, 1)], Input(2))

//# programmable --sender B --inputs 46 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

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
    { "filter": { "FromOrToAddress": { "addr": "@{A}" } } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "FromOrToAddress": { "addr": "@{B}" } } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "FromOrToAddress": { "addr": "@{C}" } } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "FromAddress": "@{A}" } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "FromAddress": "@{B}" } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "FromAddress": "@{C}" } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "FromAndToAddress": { "from": "@{A}", "to": "@{B}" } } }
  ]
}
