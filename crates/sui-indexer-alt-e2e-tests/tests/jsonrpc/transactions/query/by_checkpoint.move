// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

// 1. Output all the checkpoints, for context
// 2. Fetch transactions from an empty checkpoint
// 3. ...from a checkpoint with a single transaction
// 4. ...from a checkpoint with multiple transactions
// 5,   6,  7. ...with reversed ordering
// 8,   9, 10. ...limited to two transactions
// 11, 12, 13. ...limited and reversed
// 14. Fetch limited transactions from a checkpoint with multiple transactions,
//     with a cursor
// 17. ...reversed
// 18. Cursor points before the expected checkpoint
// 19. Cursor points after the expected checkpoint
// 20, 21. Cursor points before and after the expected checkpoint, reversed

//# create-checkpoint

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 44 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 45 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 46 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getCheckpoint",
  "params": ["0"]
}

//# run-jsonrpc
{
  "method": "sui_getCheckpoint",
  "params": ["1"]
}

//# run-jsonrpc
{
  "method": "sui_getCheckpoint",
  "params": ["2"]
}

//# run-jsonrpc
{
  "method": "sui_getCheckpoint",
  "params": ["3"]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "1" } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "2" } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "1" } },
    null, null, true
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "2" } },
    null, null, true
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } },
    null, null, true
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "1" } },
    null, 2
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "2" } },
    null, 2
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } },
    null, 2
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "1" } },
    null, 2, true
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "2" } },
    null, 2, true
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } },
    null, 2, true
  ]
}

//# run-jsonrpc --cursors 3
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } },
    "@{cursor_0}", 2
  ]
}

//# run-jsonrpc --cursors 3
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } },
    "@{cursor_0}", 2, true
  ]
}

//# run-jsonrpc --cursors 0
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } },
    "@{cursor_0}", 2
  ]
}

//# run-jsonrpc --cursors 6
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } },
    "@{cursor_0}", 2
  ]
}

//# run-jsonrpc --cursors 0
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } },
    "@{cursor_0}", 2, true
  ]
}

//# run-jsonrpc --cursors 6
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    { "filter": { "Checkpoint": "3" } },
    "@{cursor_0}", 2, true
  ]
}
