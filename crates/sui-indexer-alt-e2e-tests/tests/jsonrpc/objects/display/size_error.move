// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// Test that Display limits the overall output size (by default to 1MB).

// 1. Publish a package that includes a Display format with a potentially large output.
// 2. Create an object from this package.
// 3. Try to view a display for this object, which will fail because the output
//    is too large.

//# publish --sender A
module test::mod {
  use std::string::utf8;
  use sui::display;
  use sui::package;

  public struct MOD() has drop;

  public struct Foo has key, store {
    id: UID,
    c: Chunky<Chunky<Chunky<Chunky<u8>>>>,
  }

  public struct Chunky<T: copy + store> has copy, store {
    long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000: T,
    long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001: T,
    long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002: T,
    long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003: T,
  }

  fun init(otw: MOD, ctx: &mut TxContext) {
    let publisher = package::claim(otw, ctx);
    let mut d = display::new_with_fields<Foo>(
      &publisher,
      vector[utf8(b"c")],
      vector[utf8(b"{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}{c}")],
      ctx,
    );

    d.update_version();
    transfer::public_transfer(publisher, ctx.sender());
    transfer::public_transfer(d, ctx.sender());
  }

  public fun new(x: u8, ctx: &mut TxContext): Foo {
    Foo {
      id: object::new(ctx),
      c: chunky(chunky(chunky(chunky(x)))),
    }
  }

  public fun chunky<T: copy + store>(x: T): Chunky<T> {
    Chunky {
      long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000: x,
      long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001: x,
      long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002: x,
      long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003: x,
    }
  }
}

//# programmable --sender A --inputs 42u8 @A
//> 0: test::mod::new(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showDisplay": true }]
}
