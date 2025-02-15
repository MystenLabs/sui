// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

// 1. All objects owned by A
// 2. ...owned by B
// 3. Limited number of objects owned by A
// 4. Limited and offset by a cursor
// 5. Objects after they have been modified
// 6. Objects after they have been transferred

//# programmable --sender A --inputs @A 42 43 44
//> 0: SplitCoins(Gas, [Input(1), Input(2), Input(3)]);
//> 1: TransferObjects([NestedResult(0,0), NestedResult(0,1), NestedResult(0,2)], Input(0))

//# programmable --sender B --inputs @B 45
//> 0: SplitCoins(Gas, [Input(1)]);
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs @A 46 47 48
//> 0: SplitCoins(Gas, [Input(1), Input(2), Input(3)]);
//> 1: TransferObjects([NestedResult(0,0), NestedResult(0,1), NestedResult(0,2)], Input(0))

//# programmable --sender B --inputs @B 49
//> 0: SplitCoins(Gas, [Input(1)]);
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": ["@{A}", { "options": { "showOwner": true, "showContent": true } }]
}

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": ["@{B}", { "options": { "showOwner": true, "showContent": true } }]
}

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": ["@{A}", { "options": { "showContent": true } }, null, 2]
}

//# run-jsonrpc --cursors bcs(@{obj_4_1},2)
{
  "method": "suix_getOwnedObjects",
  "params": ["@{A}", { "options": { "showContent": true } }, "@{cursor_0}", 2]
}

//# programmable --sender B --inputs @B object(2,0) 21
//> 0: SplitCoins(Input(1), [Input(2)]);
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": ["@{B}", { "options": { "showContent": true } }]
}

//# programmable --sender B --inputs object(5,0) @A
//> 0: TransferObjects([Input(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": ["@{A}", { "options": { "showContent": true } }]
}

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": ["@{B}", { "options": { "showContent": true } }]
}
