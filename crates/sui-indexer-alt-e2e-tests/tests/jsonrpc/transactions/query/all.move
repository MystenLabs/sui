// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// 1. Default behavior of getTransactionBlock (no options)
// 2. Setting a limit
// 3. Setting a limit and cursor
// 4. Changing the order
// 5. Setting the order, cursor and limit
// 6. Providing a bad cursor
// 7. Page size too large
// 8. Unsupported filter
// 9. Supplying response options

//# programmable --sender A --inputs 12 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 34 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 56 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 78 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 90 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

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
    {},
    null,
    3
  ]
}

//# run-jsonrpc --cursors 2
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {},
    "@{cursor_0}",
    3
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {},
    null,
    null,
    true
  ]
}

//# run-jsonrpc --cursors 3
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {},
    "@{cursor_0}",
    2,
    true
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {},
    "i_am_not_a_cursor"
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {},
    null,
    10000
  ]
}

//# run-jsonrpc --cursors 1
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {
      "options": {
        "showInput": true
      }
    },
    "@{cursor_0}",
    2
  ]
}
