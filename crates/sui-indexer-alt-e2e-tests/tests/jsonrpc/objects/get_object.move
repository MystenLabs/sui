// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// 1. Fetching a freshly created object
// 2. ...the same object after it has been modified
// 3. ...after it has been wrapped
// 4. ...after it has been unwrapped
// 5. ...after it has been deleted
// 6. Fetching an object that just doesn't exist

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, sui::coin::Coin<sui::sui::SUI>>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showContent": true }]
}

//# programmable --sender A --inputs object(2,0) 21
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showContent": true }]
}

//# programmable --sender A --inputs object(1,0) 0 object(2,0)
//> 0: sui::table::add<u64, sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(1), Input(2));

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showContent": true }]
}

//# programmable --sender A --inputs object(1,0) 0 @A
//> 0: sui::table::remove<u64, sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(1));
//> 1: TransferObjects([Result(0)], Input(2))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showContent": true }]
}

//# programmable --sender A --inputs object(2,0)
//> 0: MergeCoins(Gas, [Input(0)])

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showContent": true }]
}

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"]
}
