// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// Trying to showDisplay when,
// 1. the object is a package
// 2. the type has no Display format.

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

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_1_0}", { "showDisplay": true }]
}

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showDisplay": true }]
}
