// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

// 1. Trying to fetch an object at too low a version
// 2. ...at its first version
// 3. ...after it has been modified
// 4. ...after it has been deleted
// 5. Show the details of a non-existent object verson
// 6. Show the details of an object version
// 7. Show the details of a deleted object version

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 object(1,0)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: MergeCoins(Input(1), [Result(0)])

//# programmable --sender A --inputs object(1,0)
//> 0: MergeCoins(Gas, [Input(0)])

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_1_0}", 1]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_1_0}", 2]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_1_0}", 3]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_1_0}", 4]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": [
    "@{obj_1_0}",
    1,
    {
      "showType": true,
      "showOwner": true,
      "showPreviousTransaction": true,
      "showStorageRebate": true
    }
  ]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": [
    "@{obj_1_0}",
    2,
    {
      "showType": true,
      "showOwner": true,
      "showPreviousTransaction": true,
      "showStorageRebate": true
    }
  ]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": [
    "@{obj_1_0}",
    4,
    {
      "showType": true,
      "showOwner": true,
      "showPreviousTransaction": true,
      "showStorageRebate": true
    }
  ]
}
