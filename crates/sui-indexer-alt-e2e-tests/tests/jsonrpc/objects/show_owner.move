// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

// 1. Show the owner of an object owned by one address
// 2. ...owned by another address
// 3. ...shared
// 4. ...frozen
// 5. ...owned by an object

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 44
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::transfer::public_share_object<sui::coin::Coin<sui::sui::SUI>>(Result(0))

//# programmable --sender A --inputs 45
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::transfer::public_freeze_object<sui::coin::Coin<sui::sui::SUI>>(Result(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(5,0) 46 47
//> 0: sui::table::add<u64, u64>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_1_0}", 2, { "showOwner": true }]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_2_0}", 3, { "showOwner": true }]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_3_0}", 4, { "showOwner": true }]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_4_0}", 5, { "showOwner": true }]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_6_0}", 7, { "showOwner": true }]
}
