// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --inputs 42 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# advance-clock --duration-ns 1000000000

//# programmable --inputs 42 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# advance-epoch

//# programmable --inputs 42 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --inputs 42 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --inputs 42 @A
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

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
  "method": "sui_getCheckpoint",
  "params": ["4"]
}
