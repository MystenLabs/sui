// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses Test=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 2

//# programmable --sender A --inputs 120000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 34000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 5600 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 780 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 90 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 10 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 20 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))


//# programmable --sender A --inputs 30 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# view-object 4,0

//# view-object 5,0

//# view-object 6,0

//# view-object 7,0

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, null, 3]
}

//# run-jsonrpc --cursors bcs(@{obj_2_0},1,4)
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, "@{cursor_0}"]
}

//# run-jsonrpc --cursors bcs(@{obj_7_0},1,1)
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, "@{cursor_0}"]
}

//# programmable --sender A --inputs 500 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc --cursors bcs(@{obj_1_0},2,4)
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, "@{cursor_0}"]
}

//# run-jsonrpc --cursors bcs(@{obj_1_0},1,4)
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, "@{cursor_0}"]
}

