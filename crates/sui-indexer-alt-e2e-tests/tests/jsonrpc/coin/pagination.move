// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// object(0,0) is the gas coin
// object(1,0) has a balance of 120k
// object(2,0) has 34k
// object(3,0) has 5.6k
// object(4,0) has 780
// object(5,0) has 90
// object(6,0) has 10
// object(7,0) has 20
// object(8,0) has 30

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

//# run-jsonrpc --cursors bcs(bin(0u8,@{A},0x2::coin::Coin<0x2::sui::SUI>,1u8,!34000,@{obj_2_0}))
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, "@{cursor_0}"]
}

//# run-jsonrpc --cursors bcs(bin(0u8,@{A},0x2::coin::Coin<0x2::sui::SUI>,1u8,!20,@{obj_7_0}))
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, "@{cursor_0}"]
}

//# programmable --sender A --inputs 500 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc --cursors bcs(bin(0u8,@{A},0x2::coin::Coin<0x2::sui::SUI>,1u8,!120000,@{obj_1_0}))
{
  "method": "suix_getCoins",
  "params": ["@{A}", null, "@{cursor_0}"]
}
