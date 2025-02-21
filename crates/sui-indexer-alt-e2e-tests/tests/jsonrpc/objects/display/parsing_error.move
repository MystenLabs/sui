// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// Parsing errors in the Display format will show up in the Display output, but
// will not hide the rest of the object response.

// 1. Publish a package that includes a Display format with a parsing error.
// 2. Create an object from this package.
// 3. Try to view the contents and Display of the new object -- the former will
//    succeed while the latter will not.

//# publish --sender A
module test::mod {
  use std::string::utf8;
  use sui::display;
  use sui::package;

  public struct MOD() has drop;

  public struct Foo has key, store {
    id: UID,
    bar: u64,
  }

  fun init(otw: MOD, ctx: &mut TxContext) {
    let publisher = package::claim(otw, ctx);
    let mut d = display::new_with_fields<Foo>(
      &publisher,
      // Contains a parsing error, all display requests will fail.
      vector[utf8(b"bar")],
      vector[utf8(b"{bar")],
      ctx,
    );

    d.update_version();
    transfer::public_transfer(publisher, ctx.sender());
    transfer::public_transfer(d, ctx.sender());
  }

  public fun new(bar: u64, ctx: &mut TxContext): Foo {
    Foo { id: object::new(ctx), bar }
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: test::mod::new(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showContent": true, "showDisplay": true }]
}
