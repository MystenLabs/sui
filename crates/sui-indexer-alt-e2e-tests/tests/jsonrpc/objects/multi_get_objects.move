// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

// 1. Multi-get newly created objects
// 2. ...after some have been modified
// 3. ...after some have been wrapped
// 4. ...after some have been deleted

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, sui::coin::Coin<sui::sui::SUI>>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A 42 43 44
//> 0: SplitCoins(Gas, [Input(1), Input(2), Input(3)]);
//> 1: TransferObjects([NestedResult(0,0), NestedResult(0,1), NestedResult(0,2)], Input(0))

//# programmable --sender A --inputs @A 45 46 47
//> 0: SplitCoins(Gas, [Input(1), Input(2), Input(3)]);
//> 1: TransferObjects([NestedResult(0,0), NestedResult(0,1), NestedResult(0,2)], Input(0))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_multiGetObjects",
  "params": [
    [
      "@{obj_2_0}",
      "@{obj_2_1}",
      "@{obj_2_2}",
      "@{obj_3_0}",
      "@{obj_3_1}",
      "@{obj_3_2}"
    ],
    { "showPreviousTransaction": true }
  ]
}

//# programmable --sender A --inputs object(2,0) 21
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_multiGetObjects",
  "params": [
    [
      "@{obj_2_0}",
      "@{obj_2_1}",
      "@{obj_2_2}",
      "@{obj_3_0}",
      "@{obj_3_1}",
      "@{obj_3_2}"
    ],
    { "showPreviousTransaction": true }
  ]
}

//# programmable --sender A --inputs object(1,0) 0 object(3,0)
//> 0: sui::table::add<u64, sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_multiGetObjects",
  "params": [
    [
      "@{obj_2_0}",
      "@{obj_2_1}",
      "@{obj_2_2}",
      "@{obj_3_0}",
      "@{obj_3_1}",
      "@{obj_3_2}"
    ],
    { "showPreviousTransaction": true }
  ]
}

//# programmable --sender A --inputs object(2,1)
//> MergeCoins(Gas, [Input(0)])

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_multiGetObjects",
  "params": [
    [
      "@{obj_2_0}",
      "@{obj_2_1}",
      "@{obj_2_2}",
      "@{obj_3_0}",
      "@{obj_3_1}",
      "@{obj_3_2}"
    ],
    { "showPreviousTransaction": true }
  ]
}
