// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// 1. View the contents of a package
// 2. View the contents of an arbitrary object
// 3. View the contents of a coin

//# publish
module test::mod {
  public struct Foo has key, store {
    id: UID,
  }

  public fun new(ctx: &mut TxContext): Foo {
    Foo { id: object::new(ctx) }
  }
}

//# programmable --sender A --inputs @A
//> 0: test::mod::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_1_0}", 1, { "showContent": true, "showBcs": true }]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_2_0}", 2, { "showContent": true, "showBcs": true }]
}

//# run-jsonrpc
{
  "method": "sui_tryGetPastObject",
  "params": ["@{obj_3_0}", 3, { "showContent": true, "showBcs": true }]
}
